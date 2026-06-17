import { useCallback, useEffect, useRef, useState } from "react";
import { createGame, initEngine, type Game } from "./engine";
import type { BuildingView, TickReport } from "./types";

const DT_H = 0.5; // chaque tick avance la sim de 30 min
const TICK_MS = 250; // 4 ticks/seconde
const HISTORY = 240; // ~5 jours simulés glissants

export interface GameApi {
  ready: boolean;
  report: TickReport | null;
  history: TickReport[];
  buildings: BuildingView[];
  paused: boolean;
  gridConnected: boolean;
  togglePause: () => void;
  setGridConnected: (on: boolean) => void;
  buildSolar: (kwc: number) => void;
  buildWindMicro: () => void;
  buildBattery: (kwh: number) => void;
  buildGenset: () => void;
  buildBuilding: (code: string) => void;
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
  const [paused, setPaused] = useState(false);
  const [gridConnected, setGridConnectedState] = useState(true);

  // Rafraîchit le détail des bâtiments (appareils/habitants) depuis le cœur.
  const refreshBuildings = useCallback(() => {
    const g = gameRef.current;
    if (!g) return;
    setBuildings(g.list_buildings() as BuildingView[]);
  }, []);

  useEffect(() => {
    let cancelled = false;
    initEngine().then(() => {
      if (cancelled) return;
      gameRef.current = createGame(budget, seed);
      setReady(true);
      refreshBuildings();
    });
    return () => {
      cancelled = true;
    };
  }, [budget, seed, refreshBuildings]);

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

  const buildSolar = useCallback((kwc: number) => {
    gameRef.current?.build_solar(kwc);
  }, []);
  const buildWindMicro = useCallback(() => {
    gameRef.current?.build_wind_micro();
  }, []);
  const buildBattery = useCallback((kwh: number) => {
    gameRef.current?.build_battery(kwh);
  }, []);
  const buildGenset = useCallback(() => {
    gameRef.current?.build_genset();
  }, []);
  const buildBuilding = useCallback(
    (code: string) => {
      gameRef.current?.build_building(code);
      refreshBuildings();
    },
    [refreshBuildings],
  );

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
    paused,
    gridConnected,
    togglePause,
    setGridConnected,
    buildSolar,
    buildWindMicro,
    buildBattery,
    buildGenset,
    buildBuilding,
    addApplianceTo,
    toggleAppliance,
    addResidentTo,
  };
}
