# Sprites (placeholders)

Images placeholder pixel-art chargées par la carte (`web/src/map/MapView.tsx`).
Déposez ici les fichiers PNG **avec exactement ces noms** (idéalement carrés,
fond transparent, ~64–256 px) :

| Fichier        | Élément en jeu                       |
| -------------- | ------------------------------------ |
| `solar.png`    | Panneau solaire                      |
| `wind.png`     | Micro-éolienne                       |
| `hydro.png`    | Micro-turbine hydraulique (rivière)  |
| `genset.png`   | Groupe électrogène (secours fossile) |
| `battery.png`  | Batterie de stockage                 |
| `house.png`    | Foyer / bâtiment                     |

Si un fichier est absent, la carte affiche un carré de couleur de repli — le jeu
reste jouable. Ces fichiers sont servis tels quels par Vite (dossier `public/`),
donc accessibles à l'URL `sprites/<nom>.png`.
