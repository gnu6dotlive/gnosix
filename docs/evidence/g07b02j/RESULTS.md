# G07B-02J — Cross-Output Drag Focus & Lock Lifecycle Root Cause

**12 octobre 2026 · recherche causale consolidée · G07-B production BLOCKED**

## Contexte et méthode

Référence canonique de départ : `gnosix/main@1481dd7` ; Hyprland expérimental `v0.56.2` avec patch privé `4feb578` issu de G07B-02G1 ; Aquamarine source `v0.15.1@f31c47a`. Tout le travail s'effectue dans un worktree temporaire et des compositeurs privés, sans remplacement de `/usr/bin/Hyprland`.

Les gates hérités sont G07B-02I : DnD local 2/2 PASS ; cross-output 0/2 FAIL ; lock après destruction/recréation FAIL dans au moins un essai WAYLAND-1 ; relative motion et E4 non qualifiés.

## J1 — Drag focus : causalité observée

Replay de `cross` avec deux processus Wayland natifs et une seule instance Hyprland imbriquée ayant deux outputs internes `WAYLAND-1` et `WAYLAND-2`. Le compositeur principal reste inchangé. Le parent est tracé par `WAYLAND_DEBUG=client`.

**Ordre des événements de référence :**

1. Le parent délivre `wl_pointer.enter(... wl_surface#10 ...)` sur la fenêtre de `WAYLAND-1`.
2. Le pointeur reçoit le `wl_pointer.button` `pressed` (serial `98465`).
3. Pendant la capture implicite du bouton, **38 `wl_pointer.motion`** sont dirigés vers le parent `wl_surface#10`, avec un maximum **x=2438,75**, alors que cette surface n'est large que de ~1250 px. Aucun `leave/enter` n'est émis entre `pressed` et `released`.
4. Aquamarine utilise `focusedOutput=WAYLAND-1` dans `CWaylandPointer::emitWarp()`. Le patch expérimental Hyprland observe `normalized_x=1` à la limite de ce premier output : le mouvement hors de la première fenêtre n'atteint pas la surface sur WAYLAND-2.
5. Le parent délivre `wl_pointer.button` `released` (serial `98466`), puis seulement après la capture, un `leave` de `wl_surface#10` et un `enter` vers `wl_surface#16`.
6. Les logs du client source montrent `DND_SOURCE_DRAG_STARTED` puis `DND_TARGET_DROP` sur **la source elle-même**. Le vrai client cible ne reçoit aucune `DND_DATA_OFFER`.

**Verdict J1 : PASS causal borné.** Le premier écart se produit **avant** `CWLDataDeviceProtocol::updateDrag()`, dans la projection des coordonnées de mouvement du backend parent Wayland sous grab implicite. Il serait incorrect de corriger en inventant des `dndPointerFocusChange` ou en changeant le destinataire après le relâchement : l'identité de sortie est déjà insuffisante pendant le mouvement.

Ce mécanisme explique l'échec observé dans le **banc imbriqué**. Il ne qualifie ni ne condamne le backend DRM du compositeur propriétaire direct de deux sorties physiques.
## J2 — Isolation comparative d'un autre compositeur

Weston **15.0.1** a été démarré séparément en backend `wayland` avec renderer Pixman et deux outputs 1250×800 : `wayland0` à `(0,0)` et `wayland1` à `(1250,0)`, tous deux enregistrés comme activés. Sa session et ses surfaces ont été arrêtées après le préflight.

**Résultat J2 : infrastructure indépendante bi-sortie PASS, DnD E2E indépendant NOT_RUN.** Aucun harnais contrôlant de manière indépendante les positions source/cible et les entrées du Weston parent n'a été qualifié. Le test `weston --backend=headless --output-count=2` a été écarté car `--output-count` n'est pas une option valide du backend headless dans Weston 15. Aucun test DRM/KMS isolé n'a été effectué pour préserver la session utilisateur.

Une correction transposable nécessite soit une validation sur compositor DRM/seat isolé, soit une vraie stratégie de DnD bridgé via le parent `wl_data_device`. Les coordonnées de fenêtres top-level du parent ne sont pas une donnée Wayland génériquement accessible au client; un simple dépassement de `absolute_x` ne permet donc pas de reconstruire sans ambiguïté la destination.

## J3 — Destruction et recréation de lock : comparaison appariée

Quatre exécutions contrôlées sur WAYLAND-1, avec même binaire et même fenêtre de test centrée. Chaque essai restaure le pointeur hôte; une paire utilise 6×10 px, l'autre 6×95 px.

| Mouvement hôte | Sans recréation | Avec destruction/recréation | Position interne finale |
|---|---|---|---|
| 6×10 px | 1 événement `locked` | 2 événements `locked` | Stable dans les deux cas |
| 6×95 px | 1 événement `locked` | 2 événements `locked` | Stable dans les deux cas |

**4/4 contrôles appariés stables**. Les mouvements hôtes longs totalisent environ +1124 px X, sans déplacement interne. Cette série **ne reproduit pas** le FAIL antérieur conservé dans `../g07b02i/traces/relative-lock.tar.zst`, qui avait enregistré `(271,274) → (1250,621)` après recréation et déplacement.

**Verdict J3 : ROOT CAUSE OPEN.** L'échec historique est une observation réelle, mais le mécanisme ne se déclenche pas systématiquement. Différences possibles à contrôler : état de focus/seat avant l'essai, sortie hôte, enchaînement préalable de DnD et timing de `setOnDestroy → appendConstraint → simulateMouseMovement`. Ces facteurs sont des hypothèses, **pas** des causes prouvées. Aucun changement de `CPointerConstraint` ne peut être justifié par cette série seule.
## J4 — Relative motion et décision de correction

Aquamarine `CWaylandPointer` (source `src/backend/Wayland.cpp`) relaye les `wl_pointer.motion` du parent via `emitWarp()` et les convertit en coordonnées **absolues** sur le `focusedOutput`. Le backend ne crée pas ici une source native de deltas relatifs. Hyprland émet `zwp_relative_pointer_v1.relative_motion` dans son chemin d'entrée **relative**, distinct du warp absolu. Ceci est cohérent avec l'absence d'événements relatifs observée en G07B-02I, mais **ne prouve pas l'état sur un périphérique DRM/libinput physique**.

**Décision corrective : pas de patch spéculatif dans Hyprland ou Aquamarine.** Une transformation de coordonnées fondée sur le dépassement X du grab ne serait correcte que dans une topologie hôte particulière et risquerait de router des opérations vers des surfaces non visées. La voie d'ingénierie est de qualifier séparément un backend contrôlant physiquement deux outputs, puis de concevoir un DnD parent→nested avec provenance de serial, de data offer et de seat si le support multi-output imbriqué doit devenir une fonction produit.

## J5 — Régression et provenance

`offline_regressions.py` vérifie la présence, les SHA-256 et l'intégrité des archives précédentes :

| Série antérieure | Fichiers avec intégrité vérifiée | Ce que cela démontre |
|---|---:|---|
| G07B-02G2 | 5 | 271/271 callbacks historiquement attribués |
| G07B-02H | 8 | Confinement horizontal borné, deux outputs |
| G07B-02I | 6 | DnD local 2/2, cross-output 0/2 ; échec lock initial |

**PASS sur l'intégrité documentaire, pas nouveau PASS E2E.** Aucune modification du runtime n'ayant été apportée, une matrice complète de non-régression GPU/GPUI n'est pas revendiquée.

## J6 — E4 et promotion

**E4 visuel humain : PENDING.** L'affichage des deux fenêtres expérimentales ne suffit pas à une qualification perceptuelle. Les revues utiles sont la continuité du drag au franchissement de l'écran, les changements de focus et la réactivité en mouvement, avec captures vidéo horodatées et approbation explicite. Aucune approbation humaine n'a été simulée.

**G07-B production : BLOCKED.** J1 causal PASS, J2 infrastructure Weston PASS mais comparaison DnD NOT_RUN, J3 intermittent non déterminé, J4 relative motion physique NOT_RUN, J5 intégrité PASS, J6 E4 PENDING. Aucune promotion, aucune mise à jour de `/usr/bin/Hyprland`, aucun push.

## Suite à exécuter

**G07B-02K — Physical Backend Differentiation & Host Drag Bridge Design.** Installer un banc DRM/libinput réellement isolé (pas la session de bureau), tester les deux directions de DnD et les contraintes de pointeur sous véritable `relative_motion`, puis sélectionner une réparation backend ou un pont DnD seulement si l'oracle distingue la cause. Poursuivre le suivi d'état du seat autour de la recréation de lock. Ne pas proposer G07B-03 production avant E4 et clôture des deux bugs.
