# G07B-02G — Native Two-Head & Constraint Gate

12 octobre 2026 · **DUAL-HEAD STRUCTURE PASS / POINTER ROUTING FAIL** · promotion bloquée.

## Scope

Une seule instance Hyprland expérimentale patchée et instrumentée (PID 3116911), hébergée sous Wayland sur le bureau principal inchangé. Deux sorties **simultanément actives** et dotées de coordonnées internes indépendantes :

| Sortie imbriquée | Position interne | Fenêtre hôte externe | Position fenêtre |
|---|---|---|---|
| WAYLAND-1 | 0×0, 1250×800 | HDMI-A-1 | (40,795) |
| WAYLAND-2 | 1250×0, 1250×800 | DP-2 | (2120,515) |

Les deux surfaces de qualification GPUI sont réelles, 900×560, placées sur la sortie sélectionnée. Le sélecteur explicite `G07B02_NESTED_OUTPUT` est qualifié : les bounds GPUI exposent les origines 0/1250, et la couche témoin de WAYLAND-2 est observée à x=1425.

## Résultat

| Gate | Verdict |
|---|---|
| Création de deux sorties Wayland dans un seul processus Hyprland | PASS |
| Deux fenêtres externes réellement distinctes, hôtes des sorties | PASS |
| Sélection du display GPUI par sortie | PASS |
| Matrice 16 étapes, WAYLAND-1 | **3/16 — FAIL** |
| Matrice 16 étapes, WAYLAND-2 | **0/16 — FAIL** |
| Contraintes `zwp_pointer_constraints_v1` | NOT_RUN |
| Drag-and-drop natif, E4 perceptuel | NOT_RUN |
| Promotion du correctif | **BLOCKED** |

Sur WAYLAND-1, 12 clics ne produisent aucun callback client et un arrive au mauvais destinataire. Sur WAYLAND-2, 16 clics sur 16 sont sans callback client. Le journal instrumenté du compositeur comprend **56 événements de bouton avec `pointer_focus=0`**, indiquant l'absence de client receveur à ces instants. Le mécanisme causal précis (transfert de coordonnées, reconfiguration de sortie, focus de fenêtre hôte, mapping d'input ou état de seat) **n'est pas encore déterminé**.

Le premier essai WAYLAND-2 est **invalidé** : le harnais additionnait une coordonnée de couche globale au compositeur (x=1425) à une position hôte déjà globale, menant à la cible impossible x=3995. Ce point a été corrigé en soustrayant l'origine de sortie interne, puis WAYLAND-2 a réellement été retesté. Le résultat négatif 0/16 ne provient donc pas de cette erreur de coordonnées.

## Provenance et discipline

- Compositor source : Hyprland v0.56.2 `efb50993780079460b0cbed1363e2166a2de1d9f`; instrumentation expérimentale `5a4b7e6`.
- Fixture GPUI : `1c150ac`, sélection du display nommé sans toucher au runtime Gnosix.
- `receipt.json` : hash des binaires, configuration des deux sorties, répartition des clics, 4 archives de traces et gate négatif.
- `g07b02g_twohead_stress.py` : harnais à transformation explicite `layer_global_x − inner_output_x`.
- `traces/twohead-output1.tar.zst` et `traces/twohead-output2-fixed.tar.zst` : résultats natifs négatifs. `traces/twohead-output2.tar.zst` : préflight invalide et exclu. `traces/compositor.log.zst` : journal intégral de l'instance à deux sorties.

**Ne pas extrapoler** le PASS G07B-02F : celui-ci portait sur deux bancs à sortie unique exécutés successivement. G07B-02G est le premier banc à deux sorties simultanées; il révèle une régression ou un problème de banc qui reste ouvert. Aucun merge, push, modification du compositeur principal ni déploiement.

### Next — G07B-02G1 : Native Two-Head Seat/Coordinate Focus Root Cause

Rapprocher chaque événement de pointeur de la fenêtre hôte physique, de la position dans la sortie imbriquée, de l'output courant du compositor, et de la surface ciblée par l'hit-tester. Inspecter les événements `wl_pointer.enter/motion/leave` des deux fenêtres de l'instance nested; reproduire sur une sortie unique puis ajouter la deuxième sans recréer les layers; tester des coordonnées de seat et le confinement explicite seulement après l'obtention d'un destinataire cohérent. Aucun correctif GPUI ou de production basé sur une supposition.
