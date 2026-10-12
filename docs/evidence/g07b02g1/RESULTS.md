# G07B-02G1 — Native Two-Head Seat & Coordinate Focus Root Cause

12 octobre 2026 · **Correction causale qualifiée sur deux sorties simultanées (périmètre borné)** · G07-B production **BLOCKED**.

## Point de départ

Le reçu G07B-02G `f426d63` a démontré l'existence de deux sorties Wayland simultanées dans un Hyprland expérimental, mais échouait au routage natif : **WAYLAND-1 3/16, WAYLAND-2 0/16**. Le journal du compositeur contenait 56 boutons avec `pointer_focus=0`. Les deux échecs ne pouvaient pas être reconduits à la politique d'animation.

## Preuve et cause racine n° 1 — Normalisation de pointeur, portée d'output perdue

L'audit physique (`g07b02g1-seat-positions.json`) a mis en évidence un facteur environ **×2 en X** quand les deux moniteurs imbriqués avaient chacun une largeur de 1250 px. Des valeurs locales x≈224 devenaient x≈449 dans l'espace du compositeur. La position des fenêtres parentes ayant bougé durant certaines observations, ces échantillons sont un diagnostic **et non** un oracle pixel-par-pixel à réutiliser sans leurs géométries contemporaines.

La source exacte d'Aquamarine `v0.15.1@f31c47a1` fournit `SWarpEvent.output` dans `CWaylandPointer::emitWarp`; ses coordonnées absolues sont **locales à cette sortie**. Hyprland `v0.56.2` perdait ce champ lors du relai `CMouse → IPointer::SMotionAbsoluteEvent`. `CPointerManager::warpAbsolute()` projetait alors la valeur normalisée sur la **boîte englobante de tous les moniteurs**, faute d'identité de sortie.

Le patch `4feb578` transmet le nom de sortie depuis Aquamarine jusqu'à `warpAbsolute()` et utilise la boîte logique de cette sortie; les autres périphériques sans output renseigné conservent le chemin historique.

## Cause racine n° 2 — Double soustraction de l'origine de moniteur

Le correctif de commit de région issu de G07B-02B calculait `mouse - (m_position + PMONITOR->m_position)`. Or `IHyprRenderer::arrangeLayerArray` construit déjà les géométries des layers dans les coordonnées globales du compositeur et `m_position` reprend cette géométrie. L'origine non nulle de WAYLAND-2 était donc soustraite deux fois. L'effet n'apparaissait pas sur WAYLAND-1 (origine 0).

Dans le même patch `4feb578`, les chemins de mapping et de commit utilisent désormais `mouse - m_position`. C'est un **correctif de la première expérimentation**, non une modification du code Gnosix de production.

## Vérification A/B

Même fixture GPUI, même topologie dual-head avec deux sorties et deux fenêtres hôtes simultanées, protocole layer-shell natif, fenêtre 1250×800 par output et layers superposés 900×560 :

| Sortie dans l'unique compositeur | Avant G1 | Après G1 |
|---|---:|---:|
| WAYLAND-1 (hôte HDMI-A-1) | 3/16 | **16/16** |
| WAYLAND-2 (hôte DP-2) | 0/16 | **16/16** |
| **Total** | **3/32** | **32/32** |

Chaque matrice comprend **10 transitions de membership** et **6 invariants**. Les destinataires `G07B_OVERLAY_CLICK` et `G07B_WITNESS_CLICK` sont indépendants et les régions sont vérifiées à chaque étape.

## SharedMotion réel dans la topologie simultanée

| Sortie | Politique | Callbacks observés | Clics classifiables selon l'oracle client initial |
|---|---|---:|---:|
| WAYLAND-1 | on_next_frame | 244 | 22/22 |
| WAYLAND-1 | timer 13 ms | 582 | 29/29 |
| WAYLAND-2 | on_next_frame | 250 | 24/24 |
| WAYLAND-2 | timer 13 ms | 588 | 27/27 |

Les quatre runs ont `MATCHED_COMPLETE` et 21 retargets. L'auditeur de seconde passe `g07b02g1_epoch_audit.py` analyse **135 injections** et exige un appui, un relâchement, un destinataire client horodaté, une identité de focus et une région appliquée à l'instant du press. **134/134 événements complètement attribuables sont corrects**. Neuf commits de région interviennent entre le press et le release.

**Anomalie maintenue ouverte :** sur `WAYLAND-1 / on_next_frame`, injection **n° 4**. Hyprland journalise un `press` puis un `release` avec focus valide, mais aucun des deux processus clients ne journalise `G07B_*_CLICK` dans la fenêtre d'attribution. Le cas est **non qualifié et potentiellement perdu/filtré**, pas un PASS. Cela empêche de certifier un routage 135/135.

## Provenance, négatifs et limites

- Source Hyprland ratifiée : `v0.56.2 / efb50993780079460b0cbed1363e2166a2de1d9f`; correctif expérimental `4feb578`.
- Source Aquamarine consultée : `v0.15.1 / f31c47a1b9d300847d8fc3108ad959448103dfc1`.
- `receipt.json` et `button_epoch_audit.json` portent les assertions, hash des binaires et fichiers, les détails des 135 injections et la seule exclusion.
- `g07b02g_twohead_stress.py`, `g07b02g1_twohead_motion.py`, `g07b02g1_epoch_audit.py` reproduisent la qualification ciblée. Le socket du compositeur imbriqué est choisi explicitement.
- `hyprland-output-local-abs-warp.patch.zst` (diff compressé sans altération des octets) : diff Hyprland de la seule phase G1.
- `traces/*.zst` : six archives GPUI complètes et journal instrumenté du compositeur, **7/7 vérifiées**.
- Le primaire `/usr/bin/Hyprland` et sa configuration persistent inchangés; aucune intégration Gnosix, aucun push ni promotion.
- **Non qualifiés :** confinement/verrouillage explicite, DnD, répétitions sous charge, second OS, revue perceptuelle E4, exécution du patch sur le compositeur directement propriétaire des têtes physiques.

## Décision et suite

**G07B-02G1 corrige les deux causes multi-output et franchit le gate de routage déterministe (32/32).** Les animations montrent une forte compatibilité temporelle mais un callback natif reste sans attribution. **Promotion G07-B : BLOCKED.**

**G07B-02G2 — Exact Click Delivery / Constraint Gate** : isoler l'injection n° 4 au niveau `wl_pointer.button` (trace transport) et de GPUI `on_mouse_down`, tester les conflits press/release vs nouvelle région, reproduire sous cycles supplémentaires; ensuite qualifier `zwp_pointer_constraints_v1`, la capture DnD et E4. Interdire toute mitigation par mouvement artificiel du curseur.
