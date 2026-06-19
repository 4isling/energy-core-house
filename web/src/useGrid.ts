import { useCallback, useEffect, useRef, useState } from "react";
import { createGridGame, initEngine, type GridGame } from "./engine";
import type { GridNodeView, GridSummary, NodeReport } from "./gridTypes";

const DT_H = 0.5; // chaque tick avance le réseau de 30 min
const TICK_MS = 250; // 4 ticks/seconde

export interface GridApi {
  ready: boolean;
  /** Rapports du dernier pas, indexés par NodeId (== indice). */
  reports: NodeReport[];
  /** Vues de tous les nœuds (arbre), rafraîchies après chaque action. */
  nodes: GridNodeView[];
  summary: GridSummary | null;
  root: number;
  paused: boolean;
  togglePause: () => void;
  /** Règle le tarif national (prix import/export des quartiers). */
  setNationalTariff: (importPrice: number, exportPrice: number) => void;
  /** Îlote ou reconnecte un nœud. */
  islandNode: (id: number, islanded: boolean) => void;
  /** Construit un actif sur un nœud (national/quartier). Renvoie le succès. */
  buildSolar: (id: number, kwc: number) => boolean;
  buildWind: (id: number) => boolean;
  buildBattery: (id: number, kwh: number) => boolean;
}

export function useGrid(
  seed: number,
  nDistricts: number,
  housesPerDistrict: number,
): GridApi {
  const gameRef = useRef<GridGame | null>(null);
  const [ready, setReady] = useState(false);
  const [reports, setReports] = useState<NodeReport[]>([]);
  const [nodes, setNodes] = useState<GridNodeView[]>([]);
  const [summary, setSummary] = useState<GridSummary | null>(null);
  const [root, setRoot] = useState(0);
  const [paused, setPaused] = useState(false);

  // Rafraîchit l'arbre des nœuds (après une action joueur ou un investissement).
  const refreshNodes = useCallback(() => {
    const g = gameRef.current;
    if (!g) return;
    setNodes(g.nodes() as GridNodeView[]);
  }, []);

  useEffect(() => {
    let cancelled = false;
    initEngine().then(() => {
      if (cancelled) return;
      const g = createGridGame(seed, nDistricts, housesPerDistrict);
      gameRef.current = g;
      setRoot(g.root());
      setNodes(g.nodes() as GridNodeView[]);
      setReady(true);
    });
    return () => {
      cancelled = true;
    };
  }, [seed, nDistricts, housesPerDistrict]);

  // Boucle de jeu.
  useEffect(() => {
    if (!ready || paused) return;
    const handle = setInterval(() => {
      const g = gameRef.current;
      if (!g) return;
      const r = g.tick(DT_H) as NodeReport[];
      setReports(r);
      setSummary(g.summary() as GridSummary | null);
      // Les portefeuilles et l'auto-prod NPC évoluent : rafraîchit l'arbre.
      setNodes(g.nodes() as GridNodeView[]);
    }, TICK_MS);
    return () => clearInterval(handle);
  }, [ready, paused]);

  const togglePause = useCallback(() => setPaused((p) => !p), []);

  const setNationalTariff = useCallback(
    (importPrice: number, exportPrice: number) => {
      gameRef.current?.set_national_tariff(importPrice, exportPrice);
      refreshNodes();
    },
    [refreshNodes],
  );

  const islandNode = useCallback(
    (id: number, islanded: boolean) => {
      gameRef.current?.island_node(id, islanded);
      refreshNodes();
    },
    [refreshNodes],
  );

  const buildSolar = useCallback(
    (id: number, kwc: number): boolean => {
      const ok = gameRef.current?.build_solar_on(id, kwc) ?? false;
      if (ok) refreshNodes();
      return ok;
    },
    [refreshNodes],
  );
  const buildWind = useCallback(
    (id: number): boolean => {
      const ok = gameRef.current?.build_wind_on(id) ?? false;
      if (ok) refreshNodes();
      return ok;
    },
    [refreshNodes],
  );
  const buildBattery = useCallback(
    (id: number, kwh: number): boolean => {
      const ok = gameRef.current?.build_battery_on(id, kwh) ?? false;
      if (ok) refreshNodes();
      return ok;
    },
    [refreshNodes],
  );

  return {
    ready,
    reports,
    nodes,
    summary,
    root,
    paused,
    togglePause,
    setNationalTariff,
    islandNode,
    buildSolar,
    buildWind,
    buildBattery,
  };
}
