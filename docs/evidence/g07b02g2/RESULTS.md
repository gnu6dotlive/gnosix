# G07B-02G2 — Exact Click Delivery & Event Causality

**12 octobre 2026 · qualification causale bornée : PASS · promotion G07-B : BLOCKED**

## Contrat, périmètre et provenance

Fermer l'injection native n° 4 précédemment non attribuée, distinguer le transport Wayland du callback applicatif GPUI, rejouer les deux schedulers sur une seule instance Hyprland expérimentale à deux sorties, puis sonder les contraintes explicites du pointeur. Travail réalisé dans un worktree Gnosix temporaire basé sur `main@2d319b6` ; les seuls fichiers destinés au `main` canonique sont la documentation, les auditeurs et les preuves. Aucun correctif Hyprland ou GPUI expérimental n'est intégré au runtime.

Sources expérimentales inchangées : Hyprland `v0.56.2` / `efb50993780079460b0cbed1363e2166a2de1d9f`, patch correctif multi-sortie `4feb578`, fixture GPUI historique `1c150ac`. Les quatre logs historiques appartiennent au reçu G07B-02G1 et n'ont pas été modifiés.

## G2-A/B — Injection n° 4 : une fausse absence

Le journal Hyprland contient l'appui à `1791794236385019994 ns` et le relâchement à `1791794236420086875 ns`, avec un focus valide sur l'overlay et une ressource de pointeur disponible.

Le client Wayland overlay reçoit `wl_pointer.button(152,146289562,272,1)` et `button(153,146289597,272,0)`. Le callback `G07B_OVERLAY_CLICK` est enregistré à `1791794236497525659 ns` — **112,506 ms après l'appui**. L'ancien auditeur rejetait tout callback postérieur à l'injection de plus de 12 ms ; le résultat `134/135` provenait donc d'une fenêtre d'attribution trop courte, non d'une preuve de clic perdu.

**Correction méthodologique :** `transport_audit.py` associe les appuis au temps Wayland du bouton (calibré sur l'événement original) et les relâchements selon l'ordre des événements du même client. Puis il associe chaque appui au `on_mouse_down` GPUI du destinataire par ordre de traitement. Les écarts entre le temps Wayland et la journalisation du compositeur sont conservés comme latence/ordonnancement possible, sans être traités comme une perte.

**Sur les archives originales : 135/135 appuis, 135/135 relâchements appariés et 135/135 callbacks GPUI.** L'attribution cliente du cas n° 4 est `overlay`. La localisation exacte de ses 112,5 ms (transport, scheduling ou event loop GPUI) **reste indéterminée**.

## G2-C — Reproduction simultanée à deux sorties

Un seul compositeur Hyprland patché, temporaire, présentait simultanément `WAYLAND-1` et `WAYLAND-2`, chacune 1250 × 800. Les fenêtres hôtes étaient situées respectivement sur HDMI-A-1 et DP-2. Aucune instance principale n'a été modifiée.

| Sortie et policy | Retargets terminés | Appuis Wayland | Relâchements appariés | Callbacks GPUI | Focus/région corrects à l'appui |
|---|---:|---:|---:|---:|---:|
| WAYLAND-1 / on_next_frame | 21 | 34 | 34 | 34 | **34/34** |
| WAYLAND-1 / timer 13 ms | 21 | 34 | 34 | 34 | **34/34** |
| WAYLAND-2 / on_next_frame | 21 | 34 | 34 | 34 | **34/34** |
| WAYLAND-2 / timer 13 ms | 21 | 34 | 34 | 34 | **34/34** |
| **Total replay** | | **136** | **136** | **136** | **136/136** |

Les quatre animations ont émis `MATCHED_COMPLETE`. Le nouveau maximum de délai observé entre appui compositeur et callback GPUI est **1,965 ms** ; il ne s'agit pas d'une mesure SLA/p99. L'auditeur indépendant `button_epoch_replay_audit.py` a corrélé les événements `REGION`, `FOCUS`, `BUTTON` et les destinataires clients pour **136/136** appuis. Il relève **16 frontières traversées pendant les séquences press/release**. Les trois erreurs annoncées par l'ancien oracle visuel disparaissent lorsque l'ordre de livraison effectif est pris en compte. Les traces brutes de ce replay sont archivées sous `traces/` avec hachages SHA-256.

**Total des deux séries : 271/271 appuis et callbacks attribués.** Une anomalie de latence atypique historique reste pertinente pour une qualification de longue durée.

## G2-D — Contraintes de pointeur

Le serveur imbriqué annonce `zwp_pointer_constraints_v1` version 1. Un client natif C autonome a été construit localement avec les XML système `xdg-shell` et `pointer-constraints-unstable-v1`, sans installation ni requête externe. La surface XDG utilise un vrai buffer `wl_shm`, reçoit `wl_pointer.enter`, puis soumet la requête d'activation de sa contrainte.

- **Confinement, WAYLAND-1 : PASS activation.** `G2_POINTER_ENTER`, `G2_CONSTRAINT_ACTIVE confined`, `CONSTRAINT_RESULT confine True`.
- **Verrouillage, WAYLAND-1 : PASS activation.** `G2_POINTER_ENTER`, `G2_CONSTRAINT_ACTIVE locked`, `CONSTRAINT_RESULT lock True`.
- **WAYLAND-2 : non qualifié.** Un essai a activé `confined` avant que le harnais puisse vérifier l'association de la fenêtre à la sortie cible; il est exclu de toute revendication multi-sortie.

Les protocoles ont donc **réellement activé** les contraintes sur une surface native. Cela **ne qualifie pas** le blocage effectif de la position sous mouvement prolongé, la relative motion, la fin de contrainte sous déconnexion, ni leur comportement sur la seconde sortie. Aucun test de drag-and-drop de données ou de fichier (`wl_data_device_manager`) n'a été exécuté. La revue perceptuelle E4 demeure humaine et **PENDING**.

Le premier client de test possédait un handler `wl_pointer.frame` manquant; le test a été rejeté et l'implémentation corrigée avant tout PASS. Le code `pointer_constraints_probe.c` et `build_pointer_probe.sh` permettent la reconstruction exacte (le binaire reconstruit a obtenu le même SHA-256 que le binaire testé).

## Reçus, reproductibilité et limites

- `transport_audit.py` / `transport_audit.json` : attribution indépendante des 135 événements historiques conservés dans `../g07b02g1/traces/`.
- `replay_transport_audit.py` / `replay_transport_audit.json` : attribution de 136 appuis, relâchements et callbacks dans le replay.
- `button_epoch_replay_audit.py` / `button_epoch_replay_audit.json` : qualification par région appliquée au press et identité du focus dans le même compositeur.
- `traces/*.zst` : quatre archives GPUI du replay et journal du compositeur, toutes cinq testées par `zstd -t`.
- `pointer_constraints_probe.c`, `build_pointer_probe.sh`, `confine-runner-v2.log`, `lock-runner.log`, `constraint-lock-executed.log` et `confine-output2-runner.log` : activation réelle, sources reproductibles et expérience explicitement exclue.
- `receipt.json` : schéma, SHA-256, gates et non-claims.

Aucune métrique de performance générale ne peut être déduite de quatre runs ni d'un maximum ponctuel. La corrélation d'horloges repose sur un événement identifié et doit être revalidée après migration vers une autre machine ou un autre boot. Le nombre de press/release/callback correctement appariés n'est **pas** une preuve de stabilité sous charge soutenue ni de qualification E4.

## Verdict et clôture

**PASS G2-A, G2-B, G2-C** pour l'attribution exacte et le routage sur deux sorties imbriquées. **PASS partiel G2-D** pour les deux activations de contrainte sur WAYLAND-1. **BLOCKED** pour DnD, test de mouvement contraint, WAYLAND-2 et E4. **Promotion G07-B interdite** tant que ces barrières ainsi que le gate produit indépendant ne sont pas fermés.

Les preuves sont destinées à `main` dans une intégration **documentation-only**; aucun patch Hyprland, GPUI ni configuration système ne doit être porté à la production. Le worktree d'expérimentation doit être détruit après fast-forward canonique et confirmation des empreintes.
