# G07B-02H — Pointer Constraints, DnD et E4

**12 octobre 2026 — qualification expérimentale bornée. G07-B production : BLOCKED.**

## Périmètre
Une seule instance Hyprland v0.56.2 avec les correctifs expérimentaux G07B-02G1, deux outputs Wayland imbriqués de 1250 × 800, fenêtres hôtes sur HDMI-A-1 et DP-2. Surface XDG native (buffer wl_shm) et requêtes `zwp_pointer_constraints_v1`. Le bureau principal `/usr/bin/Hyprland` est resté actif et inchangé. Aucun code Gnosix runtime n'a été modifié.

## Oracle de mouvement
Pour chaque combinaison `WAYLAND-1/WAYLAND-2 × confine/lock`, le harnais attend un véritable `wl_pointer.enter` et un événement d'activation côté protocole, puis injecte six mouvements horizontaux relatifs `ydotool` et confronte la position de pointeur signalée par **Hyprland imbriqué**, non par le curseur hôte. Le pointeur hôte est restauré après chaque exécution.

| Cas | Contrainte activée | Curseur imbriqué avant → après | Déplacement hôte X | Verdict |
|---|---|---|---|---|
| WAYLAND-1 / confine | Oui | (319,370) → (479,370) | 359 → 1483 | **PASS borné** : bord x=480 |
| WAYLAND-2 / confine | Oui | (1571,370) → (1729,375) | 2441 → 3008 | **PASS borné** : bord x=1730 |
| WAYLAND-1 / lock | Oui | (50,6) → (50,6) | 382 → 1501 | Position stable observée |
| WAYLAND-2 / lock | Oui | (1981,5) → (1981,5) | 2439 → 3008 | Position stable observée |

Pour les confinements, les deux fenêtres XDG testées sont positionnées respectivement sur `x=160..480` et `x=1410..1730` en coordonnées globales du compositeur imbriqué. Les valeurs se stabilisent aux bornes lors du mouvement forcé, avec deux sorties actives simultanément.

Pour les locks, la position interne reste fixe, **mais la correspondance avec la position locale de la surface n'a pas été démontrée**. La stabilité seule ne valide pas les événements relatifs, la sémantique de destruction ni les transitions de focus.

## Gates non franchis
- **Drag-and-drop Wayland** via `wl_data_device_manager` : **NOT_RUN**. Aucune revendication DnD ; un test presse-papiers ne serait pas un remplacement.
- **E4 perceptuel humain** : **PENDING**. Les vidéos d'origine G07B et leur page de revue restent une tâche séparée ; aucune validation visuelle n'est inventée.
- **Lock avec relative motion, résilience de déconnexion et stress prolongé** : **NOT_RUN**.
- **Promotion G07B** : **BLOCKED**, malgré l'activation 4/4 et le confinement borné 2/2.

## Reproductibilité et conservation
`pointer_constraints_probe.c` et `build_pointer_probe.sh` reproduisent le client natif. `constraint_runner.py` pilote les deux fenêtres sans déplacer la configuration persistante. Les quatre journaux `*-motion.log`, `compositor.log.zst` et `receipt.json` conservent les observations et empreintes SHA-256.

**Suite recommandée — G07B-02I** : vraie session drag-and-drop `wl_data_device_manager` avec source/cible distinctes, test de relative pointer et cycles de destruction/recréation, puis revue perceptuelle E4 par l'utilisateur. Aucun patch Hyprland expérimental intégré à la production dans cette qualification.
