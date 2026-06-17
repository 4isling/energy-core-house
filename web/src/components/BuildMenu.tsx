import { useState } from "react";
import type { GameApi } from "../useGame";
import {
  APPLIANCE_CATALOG,
  BUILDING_CATALOG,
  RESIDENT_PROFILES,
  type BuildingView,
} from "../types";

// CAPEX (cf. src/economy.rs) pour griser les boutons si budget insuffisant.
const SOLAR_KWC = 6;
const BATTERY_KWH = 10;
const COST = {
  solar: SOLAR_KWC * 1100, // 1100 €/kWc
  windMicro: 5 * 1850, // 1850 €/kW, micro = 5 kW
  battery: BATTERY_KWH * 600, // 600 €/kWh
  genset: 6 * 900, // 900 €/kW, genset = 6 kW
};

export function BuildMenu({
  game,
  budget,
}: {
  game: GameApi;
  budget: number;
}) {
  const { buildings } = game;
  const [selected, setSelected] = useState<number | null>(null);

  // Sélection courante de foyer (par défaut le premier).
  const current =
    buildings.find((b) => b.id === selected) ?? buildings[0] ?? null;

  return (
    <aside className="build-menu">
      <section>
        <h2>Micro-réseau partagé</h2>
        <BuildBtn
          label={`☀️ Solaire ${SOLAR_KWC} kWc`}
          cost={COST.solar}
          budget={budget}
          onClick={() => game.buildSolar(SOLAR_KWC)}
        />
        <BuildBtn
          label="🌬️ Micro-éolien 5 kW"
          cost={COST.windMicro}
          budget={budget}
          onClick={game.buildWindMicro}
        />
        <BuildBtn
          label={`🔋 Batterie ${BATTERY_KWH} kWh`}
          cost={COST.battery}
          budget={budget}
          onClick={() => game.buildBattery(BATTERY_KWH)}
        />
        <BuildBtn
          label="⛽ Groupe électrogène 6 kW"
          cost={COST.genset}
          budget={budget}
          onClick={game.buildGenset}
        />
      </section>

      <section>
        <h2>Construire un foyer</h2>
        {BUILDING_CATALOG.map((b) => (
          <BuildBtn
            key={b.code}
            label={`${b.emoji} ${b.label}`}
            sub={b.detail}
            cost={b.cost}
            budget={budget}
            onClick={() => game.buildBuilding(b.code)}
          />
        ))}
      </section>

      <section>
        <h2>Gérer un foyer</h2>
        {current ? (
          <ManageBuilding
            game={game}
            buildings={buildings}
            current={current}
            onSelect={setSelected}
          />
        ) : (
          <p className="muted">Construisez un foyer pour le gérer.</p>
        )}
      </section>
    </aside>
  );
}

function ManageBuilding({
  game,
  buildings,
  current,
  onSelect,
}: {
  game: GameApi;
  buildings: BuildingView[];
  current: BuildingView;
  onSelect: (id: number) => void;
}) {
  const [name, setName] = useState("Habitant");
  const [profile, setProfile] = useState(RESIDENT_PROFILES[0].code);

  return (
    <>
      <select
        className="building-select"
        value={current.id}
        onChange={(e) => onSelect(Number(e.target.value))}
      >
        {buildings.map((b) => (
          <option key={b.id} value={b.id}>
            {b.name} #{b.id} — {b.residents.length} hab.
          </option>
        ))}
      </select>

      <h3>Appareils</h3>
      <div className="catalog">
        {APPLIANCE_CATALOG.map((a) => (
          <button
            key={a.code}
            onClick={() => game.addApplianceTo(current.id, a.code)}
          >
            + {a.label}
            <span className="power">{a.power_kw} kW</span>
          </button>
        ))}
      </div>
      {current.appliances.length === 0 ? (
        <p className="muted">Aucun appareil installé.</p>
      ) : (
        <ul className="appliance-list">
          {current.appliances.map((a) => (
            <li key={a.id} className={a.on ? "on" : "off"}>
              <button onClick={() => game.toggleAppliance(a.id)}>
                <span className="dot" /> {a.name}
              </button>
              <span className="power">{a.power_kw} kW</span>
            </li>
          ))}
        </ul>
      )}

      <h3>Habitants</h3>
      <div className="resident-form">
        <input value={name} onChange={(e) => setName(e.target.value)} />
        <select value={profile} onChange={(e) => setProfile(e.target.value)}>
          {RESIDENT_PROFILES.map((p) => (
            <option key={p.code} value={p.code}>
              {p.label}
            </option>
          ))}
        </select>
        <button onClick={() => game.addResidentTo(current.id, name, profile)}>
          + Ajouter
        </button>
      </div>
      <ul className="resident-list">
        {current.residents.map((r, i) => (
          <li key={i}>
            👤 {r.name} ({r.profile}) — confort {r.comfort.toFixed(0)} %
          </li>
        ))}
      </ul>
    </>
  );
}

function BuildBtn({
  label,
  sub,
  cost,
  budget,
  onClick,
}: {
  label: string;
  sub?: string;
  cost: number;
  budget: number;
  onClick: () => void;
}) {
  const affordable = budget >= cost;
  return (
    <button className="build-btn" disabled={!affordable} onClick={onClick}>
      <span>
        {label}
        {sub && <span className="build-sub"> · {sub}</span>}
      </span>
      <span className="cost">{cost.toLocaleString("fr-FR")} €</span>
    </button>
  );
}
