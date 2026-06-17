# Construire un jeu de gestion énergétique réaliste centré sur la France : datasets open-source et modèles physiques

## TL;DR
- **Tout l'écosystème de données existe en open data réutilisable.** Le socle français (RTE éCO2mix, Enedis, Météo-France, Hub'Eau, data.gouv.fr) est sous **Licence Ouverte Etalab 2.0** (réutilisation libre, y compris commerciale), complété par PVGIS (UE, libre) et Open Power System Data (européen). Seule exception à surveiller : **Renewables.ninja est en CC BY-NC (non commercial)** — à éviter si le jeu devient commercial.
- **Les formules physiques sont simples et tabulables.** Éolien (P=½·ρ·A·v³·Cp, Betz 0,593), hydraulique (P=ρ·g·Q·H·η) et solaire (E=kWc×productible spécifique) se codent directement avec des valeurs moyennes par défaut réalistes ; calibrez-les ensuite avec les facteurs de charge réels français (nucléaire 63 % en 2023, éolien terrestre 26,2 %, solaire ~13-14 %, hydraulique variable).
- **Recommandation de démarrage** : coder d'abord le moteur avec les formules + moyennes par défaut de ce rapport (jouable sans aucun téléchargement), puis brancher progressivement les CSV éCO2mix (calibration production/conso), PVGIS (carte solaire) et Hub'Eau (débits hydro) pour le réalisme régional.

## Key Findings

1. **Données de production/consommation France** : RTE éCO2mix fournit l'historique horaire/semi-horaire 2012-2024 par filière en CSV, sous Licence Ouverte 2.0 — c'est la colonne vertébrale pour calibrer le jeu.
2. **Données météo pilotant la production** : PVGIS (irradiance + production PV par localisation, CSV horaire, libre) et Météo-France (vent, rayonnement, depuis meteo.data.gouv.fr, Licence Ouverte 2.0) suffisent pour piloter solaire et éolien. Hub'Eau couvre les débits de rivière (hydro).
3. **Physique** : les équations clés sont universelles et bien documentées ; les paramètres moyens (cut-in 3 m/s, nominale 11-13 m/s, cut-out 25 m/s, Cp réel ~0,40-0,45, rendements turbines 85-95 %) sont stables.
4. **Coûts/émissions** : la table d'émissions RTE Bilan électrique 2024 (méthodologie unique, toutes filières) et les coûts CRE PPE2 2024 / Fraunhofer ISE 2024 / IRENA 2024 donnent des valeurs CAPEX/LCOE directement exploitables.
5. **Réseau** : RTE (transport, équilibrage), Enedis (distribution), fournisseurs (EDF/ENGIE) — le mécanisme offre/demande et le marché spot day-ahead (EPEX) sont une excellente mécanique de gameplay.

## Details

### 1. MODÈLES PHYSIQUES DE PRODUCTION (formules + valeurs par défaut)

#### Éolien
**Puissance théorique dans le vent** : `P = ½ · ρ · A · v³ · Cp`
- ρ = densité de l'air ≈ **1,225 kg/m³** (niveau de la mer, 15 °C ; légèrement plus élevé offshore en air froid/dense)
- A = surface balayée = π·r² (r = rayon du rotor en m)
- v = vitesse du vent à hauteur de moyeu (m/s)
- Cp = coefficient de puissance. **Limite de Betz = 0,593** (maximum théorique absolu) ; en pratique les éoliennes modernes atteignent **Cp ≈ 0,40-0,45**.

**Courbe de puissance par régimes** (valeurs types tirées de la littérature scientifique, ex. Jerez et al. / modèles arXiv) :
- **Cut-in (démarrage)** : ~3 m/s (3-3,5 m/s)
- **Vitesse nominale (rated)** : ~11-13 m/s (la machine atteint sa puissance nominale)
- **Cut-out (coupure)** : ~25 m/s (arrêt pour protéger la machine)
- Entre cut-in et nominale : croissance en v³ ; entre nominale et cut-out : puissance plafonnée constante (le pitch réduit Cp pour limiter à la puissance nominale).

**Mise à l'échelle du vent avec la hauteur** (loi de puissance) : `V(h) = V(h0)·(h/h0)^α` avec **α ≈ 0,143 onshore** et **α ≈ 0,11 offshore**. Hauteurs de moyeu typiques : ~100 m onshore, ~150 m offshore.

**Table de courbe de puissance tabulée** (exemple normalisé pour une turbine 1 MW, exploitable directement comme lookup table dans le jeu — valeurs en kW) :
| v (m/s) | P (kW) | v (m/s) | P (kW) |
|---|---|---|---|
| 3,5 | 0 | 9 | 516 |
| 4 | 24 | 10 | 719 |
| 5 | 69 | 11 | 895 |
| 6 | 137 | 12 | 973 |
| 7 | 229 | 13 | 1000 |
| 8 | 352 | 13,5-25 | 1000 |

**Réglages jouables** : le **pitch** (angle des pales) module Cp pour plafonner la puissance ou délester en vent fort ; le **yaw** (orientation nacelle) aligne le rotor sur le vent (un désalignement de θ réduit la puissance ≈ cos³θ). Pertes de sillage en parc : ~5-10 %.

**Facteur de charge réel France (à utiliser pour calibrer)** : selon RTE (Bilan électrique 2023, chapitre Production), « le facteur de charge pour l'éolien terrestre s'est établi à **26,2 %** en 2023, contre 21,6 % en 2022 » (année peu venteuse), pour une production record de 48,9 TWh. L'offshore est plus élevé (~35-45 % attendu).

#### Hydraulique / roue à aube / turbine
**Puissance** : `P = ρ · g · Q · H · η`
- ρ = 1000 kg/m³ (eau), g = 9,81 m/s²
- Q = débit (m³/s), H = hauteur de chute nette (m), η = rendement global
- Forme pratique : **P (kW) ≈ 9,81 · Q · H · η** (Q en m³/s, H en m)

**Rendements et domaines par type de turbine** (valeurs typiques) :
| Turbine | Domaine (hauteur de chute) | Débit | Rendement de pointe |
|---|---|---|---|
| **Pelton** (impulsion) | haute chute 150-2000 m | faible | 85-92 % |
| **Francis** (réaction) | moyenne 40-600 m | moyen | jusqu'à ~95 % |
| **Kaplan** (hélice, pales réglables) | basse 2-40 m | élevé | ~90-95 %, courbe plate |
| **Roue à aube** | très basse | variable | ~60-85 % (≈60 % roues classiques) |

**Influence du débit** : Pelton et Kaplan ont une courbe de rendement « plate » (bons en charge partielle) ; Francis et hélice à pales fixes chutent vite hors du débit nominal. Pour le micro-hydro, ordres de grandeur : quelques kW à quelques centaines de kW.

#### Solaire photovoltaïque
**Production annuelle** : `E (kWh) = Puissance crête (kWc) × Productible spécifique (kWh/kWc/an)`
Forme instantanée : `P = irradiance (kW/m²) × surface × rendement module × PR`

- **Rendement des modules** : ~15-22 % (cristallin moderne)
- **Performance Ratio (PR)** : typiquement **0,75-0,85** (pertes câblage, onduleur, salissure, ombrage ; PVGIS utilise ~14 % de pertes système par défaut)
- **Coefficient de température** : la puissance baisse d'environ **−0,3 à −0,4 %/°C** au-dessus de 25 °C (température de cellule, souvent 20-30 °C au-dessus de l'air)
- **Orientation/inclinaison optimale France** : plein sud, inclinaison 30-35°
- **Irradiance France** : gradient de ~1000 kWh/m²/an (nord) à >1700 kWh/m²/an (Côte d'Azur)
- **Productible spécifique par région (à tabuler comme facteur régional)** : Paris/Île-de-France ~1000-1100 kWh/kWc/an ; Rennes/Bretagne ~1050-1150 ; Nantes ~1150-1200 ; Toulouse ~1300-1350 ; Montpellier ~1400 ; Marseille ~1400-1500 ; Nice ~1350-1450.
- **Facteur de charge France** : ~13-14 % (cohérent avec ~1100-1300 kWh/kWc/an moyens).

#### Thermique fossile
| Filière | Rendement | Émissions cycle de vie (RTE BE2024) |
|---|---|---|
| Gaz cycle combiné (CCGT) | ~55-62 % (record Bouchain 62,2 %) | 389 gCO2eq/kWh (331 direct) |
| Gaz turbine à combustion | ~35-40 % | 583 gCO2eq/kWh |
| Charbon | ~37-45 % | 941 gCO2eq/kWh |
| Fioul | ~40 % | 928 gCO2eq/kWh |

Coût combustible : pour une CCGT, le gaz peut représenter ~75 % du prix de revente de l'électricité (vs ~40 % pour le charbon, ~12 % pour le nucléaire). Hypothèses Lazard : gaz 3,45 $/MMBTU, charbon 1,47 $/MMBTU, nucléaire 0,85 $/MMBTU.

#### Nucléaire et biomasse
- **Nucléaire** : selon RTE (Bilan électrique France 2023), « la disponibilité moyenne en 2023, tous facteurs confondus, s'est élevée à 38,6 GW (**63 % du parc**), contre 33,2 GW en 2022 (54 %) », pour une production de 320,4 TWh. La filière vise un retour à « environ 75 % » de disponibilité (objectif DGEC cité dans le rapport du Sénat « Éclairer l'avenir », 2024), EDF visant ~400 TWh annuels à horizon 2030 — niveau dernièrement atteint en 2015. Émissions : **7 gCO2eq/kWh** (cycle de vie). Inertie élevée mais modulable (suivi de charge).
- **Biomasse/biogaz** : ~70 gCO2eq/kWh ; thermique pilotable.

### 2. STOCKAGE

#### Batteries lithium-ion
- **Rendement de cycle (round-trip)** : ~85-95 % (85 % valeur de référence courante pour le grid-scale ; >95 % pour cellules neuves)
- **Profondeur de décharge (DoD)** : 80-100 % pour Li-ion/LFP (limiter à 80 % allonge la durée de vie)
- **Durée de vie** : ~500-1500 cycles (NMC standard), jusqu'à **6000+ cycles** pour cellules grid-scale optimisées (LFP) ; fin de vie ≈ 70-80 % de capacité initiale
- **Coût** : selon IRENA (Renewable Power Generation Costs in 2023, sept. 2024), les coûts des projets de stockage batterie sont passés de 2 511 USD/kWh (2010) à **273 USD/kWh (2023), soit −90 %**, tandis que la capacité cumulée annuelle ajoutée passait de 0,1 GWh à 95,9 GWh. Fraunhofer ISE 2024 : système batterie 400-1000 €/kWh.
- Émissions cycle de vie (RTE) : 67 gCO2eq/kWh

#### STEP (stations de transfert d'énergie par pompage)
- **Rendement** : **70-85 %** (75-80 % installations récentes) ; pour produire 1 MWh il faut ~1,25 MWh au pompage
- **Énergie stockée** : `E = ρ·g·V·H·η` soit `E (J) ≈ 9,81 × 1000 × V × H × k` (V en m³, H en m, k≈0,80), à convertir en kWh (÷3,6×10⁶)
- **Ordres de grandeur France** : ~5 GW cumulés, ~103,5 GWh de capacité ; Grand'Maison (Isère) 1790 MW, plus puissante d'Europe, réservoir 132 millions m³, autonomie ~30 h ; bascule pompage→turbinage en <5 min
- Mécanique de jeu : pomper quand prix bas/surproduction, turbiner quand prix haut/pointe.

#### Autres
- **Hydrogène (power-to-gas-to-power)** : rendement faible, **<25-35 %**
- **Volants d'inertie** : très courte durée, haute puissance (à traiter en ordre de grandeur, sources primaires limitées)

### 3. DATASETS — CONSOMMATION (France)

**Enedis Open Data** (data.enedis.fr, Licence Ouverte 2.0) :
- **Agrégats de consommation ≤36 kVA et >36 kVA** (`conso-inf36`, `conso-sup36`, versions régionales) : courbes de charge moyennes au **pas 1/2 h**, segmentées par profil, secteur d'activité, plage de puissance. CSV/JSON. → table de profils résidentiel/tertiaire/industriel directement utilisable.
- **Coefficients des profils** (`coefficients-des-profils`) : coefficients demi-horaires réglementaires par catégorie de clientèle, validés par la CRE, historique 5 ans, mis à jour hebdo. → profils de charge normalisés clés en main (incluant le profil de production PV PRD3).
- **Simulateur de courbes de charge** : courbes synthétiques générées par IA (GAN/diffusion), datasets expérimentaux.

**Consommation moyenne (ordres de grandeur pour le jeu)** : un foyer français suit un profil thermosensible (chauffage électrique → pic hiver/soir). À calibrer via les courbes Enedis.

### 4. DATASETS — PRODUCTION RÉELLE (calibration)

**RTE éCO2mix** (Licence Ouverte 2.0) — LE jeu de données central :
- **National consolidé/définitif 2012-2024** (`eco2mix-national-cons-def`) : conso, production par filière, pompage STEP, échanges frontières, émissions CO2, au pas demi-horaire. CSV (~61 Mo) + JSON. URL : odre.opendatasoft.com / data.gouv.fr.
- **Régional** (`eco2mix-regional-cons-def`) : idem à la maille région, depuis 2013.
- **Temps réel national/régional** (`eco2mix-national-tr`) : pas 15 min, mais quota 50 000 appels API/mois (le téléchargement CSV reste possible).
→ Usage : extraire les facteurs de charge mensuels par filière, les profils saisonniers, les courbes de conso type.

**Registre national des installations de production et de stockage** (RTE, ODRE) : `registre-national-installation-production-stockage-electricite-agrege` sur odre.opendatasoft.com — parc installé par filière/territoire (jusqu'à l'IRIS), maj mensuelle, **Licence Ouverte Etalab 2.0**. Base légale art. L142-9-1 du Code de l'énergie.

**Open Power System Data** (data.open-power-system-data.org/time_series) : séries horaires conso/éolien/solaire pour pays européens dont la France, agrégées depuis ENTSO-E, en CSV téléchargeable (pas l'API). Idéal pour comparaison européenne.

**ENTSO-E Transparency Platform** : données européennes de génération/charge depuis 2015 ; téléchargement FTP/CSV recommandé (interface web lente).

**Facteurs de charge / production réels France 2023 (référence de calibration)** : nucléaire 320 TWh (63 % dispo), hydraulique 58,8 TWh, éolien 50,7 TWh (terrestre 48,9 TWh, FC 26,2 %), solaire 21,5 TWh (FC ~13-14 %), gaz 30 TWh. Mix très décarboné (~32 gCO2/kWh moyen).

### 5. DATASETS — MÉTÉO

**PVGIS** (Commission européenne JRC, joint-research-centre.ec.europa.eu) — données solaires libres et gratuites :
- Irradiance + production PV par localisation (clic carte/lat-lon), moyennes mensuelles/annuelles, **séries horaires en CSV**, Typical Meteorological Year (TMY). Données SARAH3/ERA5 jusqu'à 2023 (v5.3), incluant vent et température.
- « The data in this section are free for public use. » → directement réutilisable. Cartes par pays/région en PDF/PNG.

**Météo-France** (meteo.data.gouv.fr, **Licence Ouverte Etalab 2.0**, gratuit depuis 2024) :
- Données climatologiques de base **horaires/quotidiennes** par département (CSV compressé) : température, vent, insolation, rayonnement global, depuis l'ouverture des stations. → tables de vent et d'irradiance régionales pour piloter éolien/solaire.

**Renewables.ninja** (renewables.ninja) — ⚠️ **CC BY-NC 4.0 (non commercial)** :
- Facteurs de charge horaires PV et éolien simulés par pays européen (MERRA-2/SARAH), CSV. Très pratique mais **non utilisable si le jeu est commercialisé**. Le code GSEE/automator est open-source (MIT/BSD) mais les données restent NC.

**Hub'Eau Hydrométrie** (hubeau.eaufrance.fr, accès libre) :
- Débits (Q) et hauteurs d'eau de >5000 stations françaises ; **débits moyens journaliers et mensuels depuis 1900** (observations « élaborées »), formats JSON/CSV/GeoJSON. → alimente directement le modèle hydraulique (Q dans P=ρgQHη). API REST (export possible).

**Copernicus/ERA5** : réanalyse climatique mondiale (vent, irradiance) — source sous-jacente de Renewables.ninja, licence Copernicus libre, plus lourde à manipuler.

### 6. COÛTS ET ÉCONOMIE

**CAPEX France/Europe (sources primaires)** :
- **CRE PPE2 2024** (appels d'offres 2023, France) : éolien terrestre ~1500-2000 €/kW (~1850 observé), OPEX ~40-50 €/kW/an ; PV au sol ~935 €/kWc, OPEX ~20-25 €/kWc/an ; rooftop ~1200-1250 €/kWc.
- **Fraunhofer ISE 2024** : PV au sol 700-2000 €/kWp ; éolien terrestre 1300-1900 €/kW ; éolien offshore 2200-3400 €/kW ; batterie 400-1000 €/kWh.
- **IRENA** : coût batterie 273 USD/kWh (2023).

**LCOE (coût actualisé)** :
- **Lazard 2024** (US, $/MWh) : solaire utility 61, éolien terrestre 50, CCGT 76, batterie standalone 4 h 170-296 (hors subventions).
- **ADEME (France, données 2022)** : éolien terrestre 59 €/MWh, PV au sol 70 €/MWh, petite hydro 162 €/MWh.
- **IRENA (mondial 2023)** : PV 0,044 USD/kWh, éolien terrestre 0,033, éolien offshore 0,075.
- ⚠️ Le LCOE ne capture pas les coûts système (flexibilité, réseau, stockage) — RTE et la DGEC le soulignent ; à mentionner pour un gameplay nuancé.

**Émissions CO2 par filière — table RTE Bilan électrique 2024 (méthodologie unique, recommandée)** :
nucléaire 7 · hydraulique 6 · éolien terrestre 16 · éolien offshore 17 · solaire PV 43 · gaz CCGT 389 · gaz TAC 583 · charbon 941 · fioul 928 · biomasse/biogaz 70 · batteries 67 (gCO2eq/kWh, cycle de vie). Source : ADEME Base Empreinte (facteurs combustibles) × rendements RTE. Intensité moyenne France 2023 : ~32 gCO2/kWh.

**Prix de l'électricité** : selon RTE (Bilan électrique 2023, chapitre Prix), « le prix spot moyen annuel de l'électricité en France s'est établi à **97 €/MWh en 2023**, une division par trois par rapport au prix moyen de 2022 (276 €/MWh) et une baisse de 11 % par rapport à celui de 2021 » (plus haut 2023 : 204,9 €/MWh le 23 janvier). ARENH (nucléaire historique) : 42 €/MWh. ⚠️ **Les prix spot EPEX affichés sur éCO2mix sont propriété d'EPEX SPOT SE et réservés à un usage non commercial** — pour des prix réutilisables, passer par le dataset `wholesale-market` de data.gouv.fr ou les données ENTSO-E.

### 7. RÉSEAU ÉLECTRIQUE FRANÇAIS (inspiration gameplay)

- **RTE** : gestionnaire du réseau de transport (haute/très haute tension, ~107 000 km). Responsable de l'**équilibrage offre/demande en temps réel** (fréquence 50 Hz), du mécanisme d'ajustement et des interconnexions (export net France ~50 TWh en 2023).
- **Enedis** : gestionnaire du réseau de distribution (35 millions de clients), achemine l'électricité aux consommateurs finaux.
- **Fournisseurs (EDF, ENGIE, etc.)** : achètent/vendent l'énergie aux clients ; ne gèrent pas le réseau physique.
- **Marché spot** : EPEX Spot day-ahead (J pour J+1, 24 prix horaires) + intraday. Le prix résulte de l'empilement économique (merit order) : nucléaire/renouvelables en base (coût marginal ~nul), gaz/charbon en pointe. **Prix négatifs** lors de surproduction renouvelable : selon EPEX SPOT, **316 heures de prix Day-Ahead négatifs en 2023** (383 h en Intraday), avec un prix plancher Day-Ahead harmonisé de −500 €/MWh — excellente mécanique de jeu pour le stockage/délestage.
- **Notion de « consommation résiduelle »** (conso − production fatale solaire/éolien/fil de l'eau) : ce qui reste à couvrir par les moyens pilotables — cœur du défi d'équilibrage à reproduire dans le jeu.
- **Coût réseau** : le TURPE pèse sur la rentabilité du stockage (les STEP le paient comme consommateurs finaux).

## Recommendations

**Étape 1 — Prototype jouable sans données (jour 1 du hackathon)** : coder le moteur de simulation avec les formules physiques et les valeurs par défaut de ce rapport. Tabuler : (a) la courbe de puissance éolienne ci-dessus, (b) les rendements de turbines hydro, (c) un facteur régional solaire (kWh/kWc), (d) les rendements/coûts thermiques. Boucle de jeu : production = f(météo simulée) vs consommation = profil normalisé Enedis ; équilibrer ou subir un black-out / acheter au prix spot.

**Étape 2 — Calibration avec données historiques** : télécharger les CSV **éCO2mix national consolidé 2012-2024** (Licence Ouverte) pour extraire facteurs de charge mensuels par filière et profils saisonniers de consommation. Brancher les **profils Enedis** (coefficients réglementaires) pour des courbes de charge réalistes résidentiel/tertiaire/industriel.

**Étape 3 — Réalisme géographique** : intégrer **PVGIS** (productible solaire par région, libre) et **Météo-France** (vent/rayonnement par département, Licence Ouverte) pour que chaque emplacement de carte ait sa météo. Ajouter **Hub'Eau** (débits journaliers/mensuels) pour dimensionner les turbines hydro selon la rivière.

**Étape 4 — Couche économique** : implémenter CAPEX/OPEX (CRE PPE2 / Fraunhofer), LCOE, émissions CO2 (table RTE BE2024), et un marché spot inspiré d'EPEX (merit order + prix négatifs).

**Garde-fous licences** :
- ✅ Utiliser librement (y compris commercial) : RTE éCO2mix, Enedis, Météo-France, Hub'Eau, data.gouv.fr (Licence Ouverte Etalab 2.0), PVGIS (libre), OPSD, ENTSO-E.
- ⚠️ **Éviter en commercial** : Renewables.ninja (CC BY-NC), prix spot EPEX via éCO2mix (propriété EPEX, usage non commercial). Pour les prix, préférer le dataset `wholesale-market` de data.gouv.fr.
- 📌 **Seuils de décision** : si le projet reste un prototype de hackathon non commercial → toutes les sources sont utilisables. S'il vise une commercialisation → purger Renewables.ninja et les prix EPEX, et vérifier la mention de licence exacte de chaque dataset avant publication.

## Caveats
- **Lazard LCOE = données US** en $/MWh ; pour la France, privilégier ADEME/CRE en €/MWh. Les valeurs Lazard sont aussi critiquées (hypothèses de facteur de charge éolien jugées optimistes par certains analystes) — à prendre comme ordres de grandeur.
- **Facteurs d'émission ADEME vs RTE** : la valeur PV varie selon les versions et l'origine des panneaux (ADEME cite parfois ~25 g pour mix français, RTE 43 g) ; utiliser une seule source cohérente (recommandé : table RTE BE2024) pour éviter les incohérences.
- **Facteur de charge nucléaire 2023 (63 %) anormalement bas** (crise corrosion sous contrainte) ; l'objectif « normal » est ~75 % — choisir selon le réalisme historique voulu.
- **Renewables.ninja** est techniquement le plus pratique (facteurs de charge horaires prêts à l'emploi) mais sa licence NC est un vrai blocage commercial — d'où l'insistance sur PVGIS+Météo-France comme alternatives libres.
- Les **prix spot** historiques réutilisables nécessitent de contourner la restriction EPEX d'éCO2mix ; vérifier la licence exacte du dataset `wholesale-market`.
- Les ordres de grandeur de consommation par appareil/data center n'ont pas été détaillés faute de source primaire ciblée ; les dériver des courbes de charge Enedis agrégées.