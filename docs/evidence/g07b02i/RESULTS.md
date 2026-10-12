# G07B-02I — Native DnD, relative pointer & lock lifecycle

**12 octobre 2026 · Gate technique partiel. G07-B production = BLOCKED.**

## Objectif / cadre

Compléter G07B-02H sans substituer des gestes visuels à de véritables échanges du protocole Wayland : deux processus clients distincts pour `wl_data_device_manager`, un payload MIME borné, un drag physique maintenu puis relâché, et un client utilisant `zwp_relative_pointer_v1` avec `zwp_locked_pointer_v1` recréé.

Banc : Hyprland expérimental basé sur v0.56.2 et le correctif privé `4feb578`. Une instance imbriquée avec `WAYLAND-1` et `WAYLAND-2` actifs simultanément, à travers deux fenêtres parentes distinctes sur HDMI-A-1 et DP-2. Le binaire, les sources des clients et les archives sont identifiés en SHA-256 dans `receipt.json`. Aucun binaire du bureau principal n'a été remplacé.

## Gate DnD — véritable `wl_data_source` → `wl_data_offer`

Le binaire `dnd_native.c` est lancé séparément comme **source** et **cible**, chacune étant une surface XDG mappée avec buffer `wl_shm`. À l'appui gauche, le processus source crée `wl_data_source`, annonce `text/plain;charset=utf-8`, offre `COPY` et appelle `wl_data_device.start_drag` avec le serial du bouton. La cible ne réussit que si elle reçoit `wl_data_device.enter`, `drop`, appelle `receive`, obtient et compare les 37 octets exacts du payload. Le processus source doit ensuite confirmer `dnd_finished`.

| Parcours | Source / cible | Confirmation observée | Verdict |
|---|---|---|---|
| À l'intérieur de WAYLAND-1 | Deux processus | `drop`, 37 octets exacts, `dnd_finished` | **PASS** |
| À l'intérieur de WAYLAND-2 | Deux processus | `drop`, 37 octets exacts, `dnd_finished` | **PASS** |
| WAYLAND-1 → WAYLAND-2 | Deux processus sur sorties distinctes | Drop côté source ; aucune offre côté cible | **FAIL** |
| WAYLAND-2 → WAYLAND-1 | Deux processus sur sorties distinctes | Drop côté source ; aucune offre côté cible | **FAIL** |

Les deux échecs inter-sorties sont reproduits avec les mêmes clients, payload et protocole que les contrôles réussis. Ils sont donc un **gate négatif du banc imbriqué**. Ils **ne prouvent pas**, sans essai sur un compositeur propriétaire direct des têtes physiques, un défaut du chemin DRM ni du produit Gnosix. Le chemin à investiguer est `drag focus → fenêtre hôte Wayland → output interne → wl_data_device.enter/drop`, avec timestamps et hiérarchie des surfaces.

## Gate `zwp_relative_pointer_v1` et cycle de vie `locked_pointer`

Le client `relative_lock_probe.c` reçoit l'annonce `zwp_relative_pointer_manager_v1` et construit une vraie ressource `zwp_relative_pointer_v1` liée au `wl_pointer`. La demande initiale de lock est acceptée. Lors de la première notification `locked`, le client détruit son objet de lock puis en crée un nouveau. Les deux sorties produisent bien **deux confirmations `locked`**, sans erreur protocolaire apparente.

| Sortie | Deux confirmations `locked` | Position interne après mouvement forcé | Gate |
|---|---|---|---|
| WAYLAND-1 | Oui | (271,274) → (1250,621) ; reproduit | **FAIL continuité** |
| WAYLAND-2 | Oui | (1691,324) → (1691,324) | PASS observation bornée |

Sur WAYLAND-1, le pointeur continue de bouger malgré la seconde notification. Ce défaut a été réobservé lors d'une répétition. **Le nombre de notifications `locked` ne suffit donc pas à garantir la persistance d'un verrou effectif.**

Dans ces essais via ydotool sur le compositeur hôte, le client ne rapporte **aucun `zwp_relative_pointer_v1.relative_motion`**, malgré la création réussie de la ressource. Cette absence n'est pas une qualification négative définitive du protocole : l'entrée synthétique passe par le backend Wayland imbriqué, qui peut ne pas fournir des deltas relatifs comparables à un périphérique natif.
## E4, intégrité, clôture

**E4 perceptuel : PENDING.** Aucune revue de mouvement/capture par un observateur humain n'a été fournie. Une capture statique isolée n'aurait pas suffi à établir la fluidité des transitions, le comportement d'un drag actif ou l'absence d'anomalie à la jonction des sorties.

Les fichiers suivants permettent de reconstruire et auditer le travail :

- `dnd_native.c`, `relative_lock_probe.c`, `build_clients.sh` : clients natifs et construction reproductible avec les XML Wayland système.
- `dnd_runner.py`, `relative_runner.py` : orchestration limitée au compositeur imbriqué, ciblage explicite des fenêtres et restauration du curseur hôte. Les signatures et PID doivent être adaptés via les variables d'environnement aux futurs bancs.
- `independent_audit.py`, `audit.json` : assertions sur les quatre parcours DnD et les deux sorties du cycle de vie du lock, à partir des archives compressées.
- `traces/dnd-*.tar.zst` : logs source, cible, harnais et reçus pour quatre parcours.
- `traces/relative-lock.tar.zst` : observations de deux activations successives, échec sur WAYLAND-1 et contrôle positif borné sur WAYLAND-2.
- `traces/nested-compositor.log.zst` : journal de référence du compositeur. **6/6 archives vérifiées** par `zstd -t`.

La reconstruction a obtenu des SHA-256 identiques pour les deux clients utilisés. Les traces ont été conservées avec leurs résultats **FAIL** ; aucun retry n'est compté comme correction.

**Verdict :** DnD local 2/2 PASS ; DnD inter-sorties 0/2 FAIL ; réactivation du lock 2/2 observée, mais continuité FAIL sur WAYLAND-1 ; relative motion non qualifiée ; E4 non qualifié ; **promotion G07-B bloquée**.

**Suite proposée : G07B-02J — Cross-Output Drag Focus & Lock Lifecycle Root Cause.** Tracer le focus de drag dans le backend Aquamarine/Wayland hôte, confronter le même transfert dans une instance à deux sorties avec backend non imbriqué, caractériser la recréation de lock en gardant le pointeur centré et en injectant de vrais deltas relatifs. Ajouter E4 par revue visuelle attestée. Ne pas intégrer le patch au compositeur principal ni modifier le produit sur une hypothèse.
