# G07B-02E — Boundary Epoch & Implicit Pointer Grab

12 octobre 2026 · Hyprland `v0.56.2` / `efb50993780079460b0cbed1363e2166a2de1d9f` · **PASS borné** · G07-B production **BLOCKED**

## Objet et méthode

Qualifier, dans un compositeur Wayland imbriqué, les instants de `wl_surface.input_region` et de réévaluation de focus au contact d'un clic en cours. La version candidate de G07B-02B est conservée. Une instrumentation expérimentale, activée seulement par `G07B02E_TRACE`, ajoute trois événements Hyprland horodatés en nanosecondes : `G07B02E_REGION`, `G07B02E_FOCUS` et `G07B02E_BUTTON`. Ce sont des traces du compositeur, indépendantes des `WAYLAND_DEBUG=client` de deux processus GPUI.

Composant testé : `5a4b7e6` sur branche privée `probe/g07b02e-compositor-instrument-20261012`. Binaires identifiés dans `receipt.json`. Le compositeur primaire n'a pas été remplacé. Sortie imbriquée `WAYLAND-1` de 1250 × 800, fenêtre de test flottante temporaire, deux couches GPUI superposées de 900 × 560.

## Verdict indépendant

L'auditeur `g07b02e_audit.py` ne retient que les deux essais propres (`instrumented-grabs-v3`, `instrumented-drag-v2`), sur la base des epochs des clics clients. Il exclut les essais de préparation sans journalisation et le premier glisser, dont le nettoyage avait échoué et a été réparé séparément.

| Gate | Résultat |
|---|---|
| Transitions d'appartenance aux régions | 5 |
| Transitions corrélées à un changement de focus | **5/5 PASS** |
| Changements de région pendant un bouton maintenu | 2 |
| Événements de bouton côté compositeur | 22 |
| Scénarios clients qualifiés (deux directions de capture, drag, double clics, clics après relâchement) | **9/9 PASS** |
| Retour du curseur à sa position initiale (≤1 px) | PASS |

L'écart maximum observé entre les points de journalisation `REGION` et `FOCUS` est **57,108 µs**. Il ne qualifie pas la latence système de bout en bout, ni un SLA.

## Trace causale significative

Lors de la rétraction de l'overlay **pendant** que le bouton gauche est maintenu :

1. `G07B02E_BUTTON state=1` porte le focus initial de l'overlay.
2. `G07B02E_REGION old=true new=false` observe la région excluant la position stationnaire.
3. `G07B02E_FOCUS` pointe désormais vers le témoin sous-jacent.
4. `G07B02E_BUTTON state=0` est émis après la transition.
5. Sur le client overlay : `wl_pointer.button(pressed)`, puis `wl_pointer.button(released)` et enfin `wl_pointer.leave`.

**Important** : les pointeurs internes de focus du compositeur et le destinataire effectif du protocole Wayland ne doivent pas être confondus pendant une capture implicite. La preuve d'appartenance d'une séquence de bouton est fournie par les événements des **clients**, et non par un unique instantané de `pointerFocus`.

La direction inverse (appui sur le témoin, expansion sous le curseur, relâchement) maintient aussi la séquence client sur le témoin. Deux drags physiques traversent une frontière de région pendant l'appui (overlay → trou, trou → overlay), sans séparer leurs `press/release`. Les doubles clics sur l'overlay et le témoin livrent chacun deux paires `press/release`.

## Investigation du cas `inside=false` de G07B-02D

L'unique callback GPUI `inside=false` non crédité provenait de l'injection 5 de `patched-frame`, où l'appartenance passait de **incluse à exclue** entre les échantillons pré- et post-clic. Le dernier événement `G07B_REGION` excluant le pointeur précédait le callback d'environ **1,52 ms**, et l'avant-dernier l'incluait. Il s'agit d'un **cas de frontière traversée, non classifiable** par l'ancien oracle; ni une preuve de routage correct ni une violation démontrée. Il nécessite une instrumentation par événement dans le compositeur pour déterminer l'ordre précis d'application et de distribution.

## Intégrité et limites

- `audit.json` et `receipt.json` : fenêtres temporelles indépendantes, neuf assertions client, 5 époques de focus corrélées, hachages des sources et binaires.
- `g07b02e_grab_probe.py`, `g07b02e_drag_probe.py`, `g07b02e_audit.py` : harnais et auditeur conservés.
- `hyprland-epoch-instrument.patch.zst` : diff local compressé sans modifier ses octets du compositeur expérimental.
- `traces/instrumented-grabs-v3.tar.zst`, `traces/instrumented-drag-v2.tar.zst`, `traces/hyprland-epoch.log.zst` : événements bruts et journal complet (incluant explicitement le run exclu), vérifiés par `zstd -t`.

**Non testés :** protocole de pointeur explicitement verrouillé/confiné, glisser-déposer matériel, surfaces concurrentes, répétitions de stress longue durée, deuxième sortie physique, matrice G07-B originale à deux écrans, E4 perceptuel. Aucun push, merge, modification de l'exécutable système ni promotion du runtime.

## Suite recommandée : G07B-02F — Multi-Output & Boundary Race Qualification

Reproduire la matrice G07-B d'origine sur **HDMI-A-1 + DP-2** en isolant les fenêtres témoins, puis déclencher les clics à l'intérieur et près des frontières mobiles avec la journalisation d'époques active. Ensuite étendre à un pointeur verrouillé/confiné et à une interaction DnD explicite. Bloquer tout déploiement avant la qualification de ces gates.
