import { useState } from "react";
import type { GameApi } from "../useGame";
import {
  APPLIANCE_CATALOG,
  RESIDENT_PROFILES,
  type ApplianceView,
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
  const [name, setName] = useState("Habitant");
  const [profile, setProfile] = useState(RESIDENT_PROFILES[0].code);

  return (
    <aside className="build-menu">
      <section>
        <h2>Production</h2>
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
        <h2>Appareils</h2>
        <div className="catalog">
          {APPLIANCE_CATALOG.map((a) => (
            <button key={a.code} onClick={() => game.addAppliance(a.code)}>
              + {a.label}
              <span className="power">{a.power_kw} kW</span>
            </button>
          ))}
        </div>
        <ApplianceList
          appliances={game.appliances}
          onToggle={game.toggleAppliance}
        />
      </section>

      <section>
        <h2>Habitants</h2>
        <div className="resident-form">
          <input value={name} onChange={(e) => setName(e.target.value)} />
          <select value={profile} onChange={(e) => setProfile(e.target.value)}>
            {RESIDENT_PROFILES.map((p) => (
              <option key={p.code} value={p.code}>
                {p.label}
              </option>
            ))}
          </select>
          <button onClick={() => game.addResident(name, profile)}>
            + Ajouter
          </button>
        </div>
        <ul className="resident-list">
          {game.residents.map((r, i) => (
            <li key={i}>
              👤 {r.name} ({r.profile}) — confort {r.comfort.toFixed(0)} %
            </li>
          ))}
        </ul>
      </section>
    </aside>
  );
}

function BuildBtn({
  label,
  cost,
  budget,
  onClick,
}: {
  label: string;
  cost: number;
  budget: number;
  onClick: () => void;
}) {
  const affordable = budget >= cost;
  return (
    <button className="build-btn" disabled={!affordable} onClick={onClick}>
      {label}
      <span className="cost">{cost.toLocaleString("fr-FR")} €</span>
    </button>
  );
}

function ApplianceList({
  appliances,
  onToggle,
}: {
  appliances: ApplianceView[];
  onToggle: (id: number) => void;
}) {
  if (appliances.length === 0) {
    return <p className="muted">Aucun appareil installé.</p>;
  }
  return (
    <ul className="appliance-list">
      {appliances.map((a) => (
        <li key={a.id} className={a.on ? "on" : "off"}>
          <button onClick={() => onToggle(a.id)}>
            <span className="dot" /> {a.name}
          </button>
          <span className="power">{a.power_kw} kW</span>
        </li>
      ))}
    </ul>
  );
}
