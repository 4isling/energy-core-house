// Vue carte (PixiJS / WebGL). Le terrain 500×500 est rendu en **une seule
// texture** (1 pixel = 1 tuile) agrandie au pixel (nearest), ce qui rend le
// pan/zoom trivialement performant. Les éléments posés sont des sprites par
// dessus. Caméra (déplacement + zoom) gérée à la main pour éviter une dépendance.
import { useEffect, useRef } from "react";
import {
  Application,
  Assets,
  Container,
  Graphics,
  Sprite,
  Texture,
} from "pixi.js";
import type { BuildTool, PlacementView, TerrainData } from "../types";

const TILE = 16; // px par tuile à zoom 1
const SPRITE_FILES = ["wind", "solar", "hydro", "genset", "battery", "house"];

// Couleur RGBA d'une tuile selon sa nature (et facteurs vent/soleil/eau).
function tileColor(ground: number, sun: number, water: number): [number, number, number] {
  switch (ground) {
    case 0: {
      // Eau : du turquoise (petit débit) au bleu profond (grosse rivière).
      const t = water / 255;
      return [30 + 20 * (1 - t), 90 + 60 * (1 - t), 170 + 60 * t];
    }
    case 2: // Forêt
      return [38, 102, 52];
    case 3: // Colline
      return [150, 132, 92];
    case 4: // Montagne
      return [142, 142, 152];
    default: {
      // Plaine : verdure modulée par l'ensoleillement (ombrage = plus sombre).
      const s = 0.7 + 0.3 * (sun / 255);
      return [118 * s, 176 * s, 88 * s];
    }
  }
}

// Construit la texture de terrain (une image mapW×mapH).
function buildTerrainTexture(terrain: TerrainData): Texture {
  const { width, height, ground, solar, water } = terrain;
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext("2d")!;
  const img = ctx.createImageData(width, height);
  for (let i = 0; i < width * height; i++) {
    const [r, g, b] = tileColor(ground[i], solar[i], water[i]);
    const o = i * 4;
    img.data[o] = r;
    img.data[o + 1] = g;
    img.data[o + 2] = b;
    img.data[o + 3] = 255;
  }
  ctx.putImageData(img, 0, 0);
  const tex = Texture.from(canvas);
  tex.source.scaleMode = "nearest"; // pas de flou en agrandissant
  return tex;
}

// Texture de repli (carré coloré) si un sprite placeholder est absent.
function fallbackTexture(kind: string): Texture {
  const g = new Graphics();
  const colors: Record<string, number> = {
    wind: 0xffffff,
    solar: 0x1f4ed8,
    hydro: 0x2563eb,
    genset: 0xf59e0b,
    battery: 0x16a34a,
    house: 0xb45309,
  };
  g.roundRect(0, 0, 64, 64, 10).fill(colors[kind] ?? 0x888888);
  const tex = (globalThis as any).__pixiApp?.renderer?.generateTexture(g);
  return tex ?? Texture.WHITE;
}

interface Props {
  terrain: TerrainData;
  placements: PlacementView[];
  selectedTool: BuildTool | null;
  onPlace: (x: number, y: number) => void;
  onHoverTile: (x: number, y: number) => void;
}

export function MapView({ terrain, placements, selectedTool, onPlace, onHoverTile }: Props) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const appRef = useRef<Application | null>(null);
  const worldRef = useRef<Container | null>(null);
  const placeLayerRef = useRef<Container | null>(null);
  const hiliteRef = useRef<Graphics | null>(null);
  const texRef = useRef<Map<string, Texture>>(new Map());
  // Valeurs « live » lues par les handlers Pixi sans recréer la scène.
  const toolRef = useRef<BuildTool | null>(selectedTool);
  const placeCbRef = useRef(onPlace);
  const hoverCbRef = useRef(onHoverTile);
  toolRef.current = selectedTool;
  placeCbRef.current = onPlace;
  hoverCbRef.current = onHoverTile;

  // Init Pixi une seule fois (terrain fixe pour une partie).
  useEffect(() => {
    let destroyed = false;
    const host = hostRef.current!;
    const app = new Application();

    app.init({ background: 0x0b1020, antialias: false, resizeTo: host }).then(async () => {
      if (destroyed) {
        app.destroy(true);
        return;
      }
      appRef.current = app;
      (globalThis as any).__pixiApp = app;
      host.appendChild(app.canvas);

      const world = new Container();
      app.stage.addChild(world);
      worldRef.current = world;

      // Terrain : une grosse texture agrandie.
      const terrainTex = buildTerrainTexture(terrain);
      const terrainSprite = new Sprite(terrainTex);
      terrainSprite.width = terrain.width * TILE;
      terrainSprite.height = terrain.height * TILE;
      world.addChild(terrainSprite);

      const placeLayer = new Container();
      world.addChild(placeLayer);
      placeLayerRef.current = placeLayer;

      const hilite = new Graphics();
      world.addChild(hilite);
      hiliteRef.current = hilite;

      // Charge les sprites placeholder (repli si absents).
      const base = import.meta.env.BASE_URL;
      for (const name of SPRITE_FILES) {
        try {
          const tex = await Assets.load(`${base}sprites/${name}.png`);
          texRef.current.set(name, tex);
        } catch {
          texRef.current.set(name, fallbackTexture(name));
        }
      }
      if (!destroyed) drawPlacements();

      // Caméra : centrée, zoom pour voir une bonne portion de carte.
      const fit = Math.max(0.05, Math.min(host.clientWidth, host.clientHeight) / (60 * TILE));
      world.scale.set(fit);
      world.position.set(
        host.clientWidth / 2 - (terrain.width * TILE * fit) / 2,
        host.clientHeight / 2 - (terrain.height * TILE * fit) / 2,
      );

      setupInteraction(app, world);
    });

    return () => {
      destroyed = true;
      appRef.current?.destroy(true);
      appRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [terrain]);

  // Redessine les sprites posés quand la liste change.
  useEffect(() => {
    drawPlacements();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [placements]);

  function spriteFor(kind: PlacementView["kind"]): Texture {
    const file = kind === "building" ? "house" : kind;
    return texRef.current.get(file) ?? Texture.WHITE;
  }

  function drawPlacements() {
    const layer = placeLayerRef.current;
    if (!layer) return;
    layer.removeChildren();
    for (const p of placements) {
      const s = new Sprite(spriteFor(p.kind));
      s.width = TILE;
      s.height = TILE;
      s.position.set(p.x * TILE, p.y * TILE);
      layer.addChild(s);
    }
  }

  function setupInteraction(app: Application, world: Container) {
    const stage = app.stage;
    stage.eventMode = "static";
    stage.hitArea = app.screen;

    let dragging = false;
    let moved = 0;
    let last = { x: 0, y: 0 };

    const toTile = (gx: number, gy: number) => {
      const local = world.toLocal({ x: gx, y: gy });
      return { tx: Math.floor(local.x / TILE), ty: Math.floor(local.y / TILE) };
    };

    stage.on("pointerdown", (e) => {
      dragging = true;
      moved = 0;
      last = { x: e.global.x, y: e.global.y };
    });
    stage.on("pointerup", (e) => {
      dragging = false;
      if (moved < 5) {
        const { tx, ty } = toTile(e.global.x, e.global.y);
        if (tx >= 0 && ty >= 0 && tx < terrain.width && ty < terrain.height) {
          placeCbRef.current(tx, ty);
        }
      }
    });
    stage.on("pointerupoutside", () => {
      dragging = false;
    });
    stage.on("pointermove", (e) => {
      if (dragging) {
        const dx = e.global.x - last.x;
        const dy = e.global.y - last.y;
        moved += Math.abs(dx) + Math.abs(dy);
        world.position.x += dx;
        world.position.y += dy;
        last = { x: e.global.x, y: e.global.y };
      } else {
        const { tx, ty } = toTile(e.global.x, e.global.y);
        updateHilite(tx, ty);
        if (tx >= 0 && ty >= 0 && tx < terrain.width && ty < terrain.height) {
          hoverCbRef.current(tx, ty);
        }
      }
    });

    // Zoom molette centré sur le pointeur.
    app.canvas.addEventListener(
      "wheel",
      (ev) => {
        ev.preventDefault();
        const factor = ev.deltaY < 0 ? 1.15 : 1 / 1.15;
        const next = Math.min(4, Math.max(0.03, world.scale.x * factor));
        const rect = app.canvas.getBoundingClientRect();
        const px = ev.clientX - rect.left;
        const py = ev.clientY - rect.top;
        const before = world.toLocal({ x: px, y: py });
        world.scale.set(next);
        const after = world.toLocal({ x: px, y: py });
        world.position.x += (after.x - before.x) * next;
        world.position.y += (after.y - before.y) * next;
      },
      { passive: false },
    );
  }

  function updateHilite(tx: number, ty: number) {
    const h = hiliteRef.current;
    if (!h) return;
    h.clear();
    if (tx < 0 || ty < 0 || tx >= terrain.width || ty >= terrain.height) return;
    const tool = toolRef.current;
    let color = 0xffffff;
    if (tool) {
      const ground = terrain.ground[ty * terrain.width + tx];
      const okTerrain = tool.terrain === "water" ? ground === 0 : ground !== 0 && ground !== 4;
      color = okTerrain ? 0x49d17a : 0xe2483c;
    }
    h.rect(tx * TILE, ty * TILE, TILE, TILE).stroke({ width: 2, color });
  }

  return <div className="map-view" ref={hostRef} />;
}
