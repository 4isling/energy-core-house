import { useCallback, useEffect, useRef, useState } from "react";
import { createGame, initEngine, type Game } from "./engine";
import type {
  BuildingView,
  BuildTool,
  PlacementView,
  TerrainData,
  TickReport,
  TileInfo,
} from "./types";

const DT_H = 0.5; // chaque tick avance la sim de 30 min
const TICK_MS = 250; // 4 ticks/seconde
const HISTORY = 240; // ~5 jours simulés glissants

export interface GameApi {
  ready: boolean;
  report: TickReport | null;
  history: TickReport[];
  buildings: BuildingView[];
  terrain: TerrainData | null;
  placements: PlacementView[];
  paused: boolean;
  gridConnected: boolean;
  togglePause: () => void;
  setGridConnected: (on: boolean) => void;
  /** Pose un outil sur une tuile. Renvoie 0 si OK, sinon un code d'erreur
   * (1 budget, 2 hors carte, 3 occupée, 4 terrain, 5 code inconnu). */
  placeAt: (tool: BuildTool, x: number, y: number) => number;
  tileInfo: (x: number, y: number) => TileInfo | null;
  addApplianceTo: (buildingId: number, code: string) => void;
  toggleAppliance: (id: number) => void;
  addResidentTo: (buildingId: number, name: string, profile: string) => void;
}

export function useGame(budget: number, seed: number): GameApi {
  const gameRef = useRef<Game | null>(null);
  const [ready, setReady] = useState(false);
  const [report, setReport] = useState<TickReport | null>(null);
  const [history, setHistory] = useState<TickReport[]>([]);
  const [buildings, setBuildings] = useState<BuildingView[]>([]);
  const [terrain, setTerrain] = useState<TerrainData | null>(null);
  const [placements, setPlacements] = useState<PlacementView[]>([]);
  const [paused, setPaused] = useState(false);
  const [gridConnected, setGridConnectedState] = useState(true);

  // Rafraîchit le détail des bâtiments (appareils/habitants) depuis le cœur.
  const refreshBuildings = useCallback(() => {
    const g = gameRef.current;
    if (!g) return;
    setBuildings(g.list_buildings() as BuildingView[]);
  }, []);

  // Rafraîchit la liste des éléments posés sur la carte (pour le rendu).
  const refreshPlacements = useCallback(() => {
    const g = gameRef.current;
    if (!g) return;
    setPlacements(g.list_placements() as PlacementView[]);
  }, []);

  useEffect(() => {
    let cancelled = false;
    initEngine().then(() => {
      if (cancelled) return;
      const g = createGame(budget, seed);
      gameRef.current = g;
      // Charge le terrain une seule fois (tableaux d'octets compacts).
      setTerrain({
        width: g.map_width(),
        height: g.map_height(),
        ground: g.terrain_ground(),
        wind: g.terrain_wind(),
        solar: g.terrain_solar(),
        water: g.terrain_water(),
      });
      setReady(true);
      refreshBuildings();
      refreshPlacements();
    });
    return () => {
      cancelled = true;
    };
  }, [budget, seed, refreshBuildings, refreshPlacements]);

  // Boucle de jeu.
  useEffect(() => {
    if (!ready || paused) return;
    const handle = setInterval(() => {
      const g = gameRef.current;
      if (!g) return;
      const r = g.tick(DT_H) as TickReport;
      setReport(r);
      setHistory((h) => {
        const next = h.length >= HISTORY ? h.slice(1) : h.slice();
        next.push(r);
        return next;
      });
      refreshBuildings(); // les habitants ont pu changer l'état des appareils
    }, TICK_MS);
    return () => clearInterval(handle);
  }, [ready, paused, refreshBuildings]);

  const togglePause = useCallback(() => setPaused((p) => !p), []);

  const setGridConnected = useCallback((on: boolean) => {
    gameRef.current?.set_grid_connected(on);
    setGridConnectedState(on);
  }, []);

  // Pose un outil sur une tuile (clic carte). Renvoie 0 si OK, sinon un code.
  const placeAt = useCallback(
    (tool: BuildTool, x: number, y: number): number => {
      const g = gameRef.current;
      if (!g) return 2;
      let code: number;
      if (tool.category === "building") {
        const r = g.build_building_at(x, y, tool.buildingCode ?? tool.id);
        code = r >= 0 ? 0 : -r; // id>=0 = OK, sinon code d'erreur
      } else {
        switch (tool.id) {
          case "solar":
            code = g.build_solar_at(x, y, 6.0);
            break;
          case "wind":
            code = g.build_wind_at(x, y);
            break;
          case "hydro":
            code = g.build_hydro_at(x, y);
            break;
          case "genset":
            code = g.build_genset_at(x, y);
            break;
          case "battery":
            code = g.build_battery_at(x, y, 10.0);
            break;
          default:
            code = 5;
        }
      }
      if (code === 0) {
        refreshBuildings();
        refreshPlacements();
      }
      return code;
    },
    [refreshBuildings, refreshPlacements],
  );

  const tileInfo = useCallback((x: number, y: number): TileInfo | null => {
    const g = gameRef.current;
    if (!g) return null;
    try {
      return g.tile_info(x, y) as TileInfo;
    } catch {
      return null; // hors carte
    }
  }, []);

  const addApplianceTo = useCallback(
    (buildingId: number, code: string) => {
      gameRef.current?.add_appliance_to(buildingId, code);
      refreshBuildings();
    },
    [refreshBuildings],
  );
  const toggleAppliance = useCallback(
    (id: number) => {
      gameRef.current?.toggle_appliance(id);
      refreshBuildings();
    },
    [refreshBuildings],
  );
  const addResidentTo = useCallback(
    (buildingId: number, name: string, profile: string) => {
      gameRef.current?.add_resident_to(buildingId, name, profile);
      refreshBuildings();
    },
    [refreshBuildings],
  );

  return {
    ready,
    report,
    history,
    buildings,
    terrain,
    placements,
    paused,
    gridConnected,
    togglePause,
    setGridConnected,
    placeAt,
    tileInfo,
    addApplianceTo,
    toggleAppliance,
    addResidentTo,
  };
}
