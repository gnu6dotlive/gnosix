# gnosix : audit des mises à jour de dépendances et des composants amont (2026-11-11)

Dépôt : gnosix, branche claude/lucid-bell-igrae7, HEAD 079b167. Lecture seule : aucun fichier du dépôt modifié ; `cargo update` et `cargo audit` ont tourné dans une copie jetable.

## Résumé : les 5 points qui comptent maintenant

1. **Hyprland 0.57.2 est sorti le 2026-11-09** (tag `v0.57.2` = `11cbc6c5`), deux jours après la 0.57.1 du poste (`acc1aa0`, vérifié : c'est bien le commit du tag `v0.57.1`). La liste exacte `SUPPORTED_HYPRLAND_VERSIONS = ["0.56.2", "0.57.1"]` **rejettera donc une 0.57.2** dès que nixpkgs ou le flake du poste avancera (`is_supported_hyprland_version("0.57.2")` vaut false, ce qui bloque le backend d'activation et `production_host::verify_session`). La 0.57.2 ne touche ni `hyprctl`, ni `src/ipc`, ni les requêtes Lua (`hl.*`). En revanche, elle change trois comportements qui concernent directement gnosix :
   - layer-shell : au map, `simulateMouseMovement` est remplacé par `refocusPointer` (#16902). C'est le focus pointeur sur une layer qui vient d'être mappée, donc pertinent pour les investigations G07/G07B sur le focus et la région d'entrée ;
   - les layers bottom et background ne sont plus masquées sous un fullscreen ; seules les layers top sans `above_fullscreen` le sont (#16740) ;
   - xdg-activation : des tokens « urgent-only » sont désormais émis quand le serial manque (#16827). Cela touche les chemins d'activation (in-gnu-os-activation, gpui).

   Autres correctifs : seat grab vérifié contre le session-lock (#16900, pertinent pour gnos-lock), une ref expirée dans `window.destroy` côté Lua (#16853), subcompositor.
2. **Hyprland `main`, après 0.57.2, prépare des ruptures IPC** (commit `6170b35b`, « remove Hyprlang references », #16889) :
   - suppression de la commande `hyprctl keyword` ;
   - suppression des champs `buildHyprlang` et `systemHyprlang` de `hyprctl -j version` ;
   - changement de format de `getoption` pour les valeurs vec2, str et custom.

   gnosix n'utilise ni `keyword` ni `getoption`, et ses parseurs de `version` sont textuels (`hyprctl_version_text_is_supported`) ou ne lisent pas ces champs. **Impact attendu nul, mais à requalifier** à la prochaine version mineure. Autres changements de `main`, tous additifs : `sysname` dans `j/devices` (#16652, que le parseur `keyboard_lock` tolère car il lit un `Value`), `hl.workspace.show` et les helpers de scrolling Lua.
3. **GPUI vendoré : 7 snapshots de retard.** La copie vendorée est `gpui-pre 0.3.5` (= zed@d89e9c2, 2026-09-14) ; la dernière est `gpui-pre 0.3.12` (= zed@d2a4fb6, 2026-11-09). Amont 0.3.12 :
   - **ne corrige toujours pas** la réécriture de la taille layer-shell 0 (`layer_surface.set_size(width, height)` est intact dans `set_geometry`) ;
   - n'a toujours pas de `set_keyboard_interactivity` à l'exécution ni les formes de curseur CURSOR-01. Les patches gnosix restent donc nécessaires ;
   - **intègre désormais `ext-background-effect-v1`** (`set_blur_region`), qui recouvre le seam « backdrop-blur regions » de gnosix (conflit de rebase probable) ;
   - retravaille xdg-activation (`ActivationContext`) ;
   - passe à **wgpu 30**, ce qui supprimerait la double pile wgpu 29 + 30 actuelle (wgpu 29 vient de gpui-pre-wgpu 0.3.5, wgpu 30 de gnoshow).

   `layer_shell.rs` et `set_input_region` (avec son commit immédiat) sont inchangés entre 0.3.5 et 0.3.12.
4. **zbus 6 est sorti** (6.0.0 le 2026-10-16, 6.2.1 le 2026-11-01). C'est une rupture : **suppression de l'API blocking**, renommage `Value` en `Variant`, `zvariant` et `zbus_names` fusionnés dans zbus. RustSec marque déjà `zvariant`, `zvariant_derive` et `zbus_names` 5.x comme « unmaintained ». gnosix utilise `zbus::blocking` dans 6 crates et `zvariant::Value` 22 fois. Migration non triviale, et bloquée en pratique par `ashpd =0.13.13`, dont gpui-pre-linux 0.3.12 dépend encore en 0.13 (zbus 5) ; ashpd 0.14 passe à zbus 6. **Piège** : un `cargo update` naïf fait passer `notify-rust` de 4.18.0 à 4.19.0 (semver-compatible), qui exige zbus ^6, et **ajoute zbus 6.2.1 + zruntime en doublon** dans l'arbre via gpui-pre-linux.
5. **Toolchain et RustSec.** 1.97.0 est épinglé ; la dernière stable sur le canal est **1.99.1** (2026-10-15). 1.97.1 corrige une miscompilation LLVM et 1.99.1 une miscompilation des retours `bool` FFI sur x86_64. 1.100.0 est annoncée au 2026-11-12 dans RELEASES.md mais n'était pas encore sur le canal stable. `cargo audit` sur le Cargo.lock actuel ne trouve **aucune vulnérabilité**, seulement 10 avertissements, dont deux « unsound » corrigeables par des mises à jour semver-compatibles :
   - tokio 1.53.2, RUSTSEC-2026-0386 (`AsyncFd::new`) : passer à ≥ 1.53.3 ;
   - rustix 1.1.5, RUSTSEC-2026-0486 (xattr) : passer à ≥ 1.1.6.

   Après un `cargo update` dans la copie, ces deux avertissements disparaissent.

## Faits (avec source)

### Inventaire
- Toolchain : `rust-toolchain.toml` → `channel = "1.97.0"`, rustfmt et clippy, profil minimal. 1.97.1 et nightly sont installés sur l'hôte mais non qualifiés, d'après le commentaire du fichier.
- Pas de `[workspace.dependencies]`. Chaque crate déclare ses dépendances.
- `[patch.crates-io]` : `gpui-pre-linux` → `vendor/gpui-pre-linux-0.3.5` (taille layer-shell 0 conservée, formes de curseur CURSOR-01) et `gpui-pre` → `vendor/gpui-pre-0.3.5` (régions de blur typées, variantes `CursorStyle`).
- `vendor/` :
  - `gpui-pre-0.3.5` : « gpui-pre snapshot of zed@d89e9c2 », d'après la description dans Cargo.toml.
  - `gpui-pre-linux-0.3.5` : diff avec le .crate officiel 0.3.5 sur `platform.rs`, `wayland.rs`, `wayland/client.rs` (~215 lignes) et `wayland/window.rs` (~303 lignes). Pour `gpui-pre`, diff sur `platform.rs` et `window.rs`.
  - `blobin-ui-core` 0.1.0 : dépôt privé `blob.in-showcase` à la révision `5c963b61…` (VENDORED.md). Pas d'amont public ; NON VÉRIFIÉ pour une version plus récente.
- Les autres crates gpui (`gpui-pre-platform`, `gpui-pre-wgpu`, etc.) viennent de crates.io, épinglées `=0.3.5`.

### Hyprland (source : `git ls-remote` et `git log` sur https://github.com/hyprwm/Hyprland ; l'API GitHub releases était refusée par le proxy de session, donc le texte des notes de version n'a pas été lu)
- Tags : v0.56.2 `efb50993`, v0.57.0 `0aa6745f`, v0.57.1 `acc1aa0b` (« [gha] Nix: update inputs »), **v0.57.2 `11cbc6c5` (2026-11-09)**. Aucun tag 0.58. HEAD de `main` au 2026-11-11 : `70ec30e7`, 67 commits après v0.57.2.
- `git diff --stat v0.57.1 v0.57.2` : aucun fichier sous `hyprctl/`, `src/ipc/` ni `LuaBindingsQuery`. Fichiers touchés : `LayerSurface.cpp`, `XDGActivation.cpp`, `SeatManager.cpp`, `LuaWindow.cpp`, `LuaEventHandler.cpp`, keybinds, subcompositor.
- Commits clés de v0.57.2 : f0d38186 (#16902, refocusPointer au map d'une layer), b280c55a (#16740, layers sous fullscreen), e65d6d13 (#16827, xdg-activation), 8a065917 (#16900, seat grab et session lock), a29e91e9 (#16853, Lua).
- Commits clés de `main` après v0.57.2 : 6170b35b (#16889, suppression de `hyprctl keyword`, des champs `*Hyprlang` de `version` et du format `getoption`), 15b8b8e8 (#16652, `sysname` dans les devices), f9f9157a (#16331, `hl.workspace.show`), 988b9436 (#16811, iostreams remplacés par println), 815585bb (#16885, focus à la fermeture d'une fenêtre flottante).
- Usages Lua de gnosix relevés dans le code : `hl.version`, `hl.dispatch`, `hl.dsp.focus`, `hl.dsp.window.move`, `hl.dsp.exit`, `hl.dsp.dpms`, `hl.monitor`, `hl.bind`, `hl.layer_rule`, `hl.get_layers`, `hl.get_config`, `hl.exec_cmd`, `hl.config`, via `hyprctl repl` et `hyprctl eval`. Aucun n'est retiré par les diffs v0.57.1..main lus (dispatchers, Query, LuaWindow, LuaDevice).
- IPC socket gnosix : `j/monitors`, `j/devices`, `j/clients`, etc. (`observation_transport.rs`). Le parseur `keyboard_lock` (« hyprctl -j devices for Hyprland 0.56 ») lit `keyboards[].main`, `capsLock` et `numLock`, que les diffs ne touchent pas.

### Rust (sources : https://static.rust-lang.org/dist/channel-rust-stable.toml et https://raw.githubusercontent.com/rust-lang/rust/stable/RELEASES.md)
- Stable du canal : 1.99.1 (58cf8f9c3, 2026-10-13 ; RELEASES.md la date du 2026-10-15). Versions précédentes : 1.99.0, 1.98.1, 1.98.0, 1.97.1.
- 1.97.1 : « Fix miscompilation in LLVM optimization ». 1.99.1 : « fix miscompilation of FFI `bool` return values on x86_64 ».
- MSRV : zbus 6 et ashpd 0.14 exigent 1.89, smithay-client-toolkit 0.21.1 exige 1.86, tokio 1.54 exige 1.85. Tous compatibles avec 1.97.0.

### RustSec (cargo-audit 0.23.0 installé dans le scratchpad, advisory-db cloné le 2026-11-11)
- Vulnérabilités : 0.
- Unsound :
  - tokio 1.53.2, RUSTSEC-2026-0386, corrigé dans >=1.53.3 ou >=1.51.6,<1.52 ;
  - rustix 1.1.5, RUSTSEC-2026-0486, corrigé dans >=1.1.6.
- Unmaintained, sans correctif :
  - zvariant, zvariant_derive, zbus_names (RUSTSEC-2026-0370, 0372, 0371) : remplacés par zbus 6 ;
  - ttf-parser 0.25.1 (RUSTSEC-2026-0192 ; dépendance directe optionnelle de gnos-ux-gpui ; alternative proposée : skrifa) ;
  - rustybuzz 0.20.1 (RUSTSEC-2026-0206) ;
  - paste 1.0.15 (RUSTSEC-2024-0436).
- Yanked : async-executor 1.14.0, yoke-derive 0.8.3. Les deux sont mis à jour par `cargo update`.

### `cargo update` dans la copie
- 116 paquets verrouillés à la dernière version compatible. Mises à jour notables : tokio 1.54.1, rustix 1.1.6, serde_json 1.0.154, uuid 1.29.2, calloop 0.14.5, libc 0.2.191, thiserror 2.0.21, zvariant 5.15.1, toml 1.2.1 (transitive).
- **Ajout de zbus 6.2.1, zbus_macros 6.2.1, zbus_utils et zruntime** via `notify-rust 4.19.0`, qui dépend de `gpui-pre-linux`.
- Doublons existants (`cargo tree -d`, cible linux) : wgpu et naga 29 + 30, thiserror 1 + 2, pollster 0.2 + 0.4, syn 2 + 3, png, skrifa et read-fonts en deux versions, roxmltree, hashbrown ×3.

## Tableau

Les dates de publication et les versions « dernière » viennent de l'API crates.io (`https://crates.io/api/v1/crates/<nom>`), interrogée le 2026-11-11.

| Composant | Verrouillé / vendoré | Dernière | Rupture ? | Pertinence | Source |
|---|---|---|---|---|---|
| Hyprland | 0.57.1 (poste, acc1aa0) ; code : 0.56.2 qualifiée, 0.57.1 candidate | **0.57.2** (2026-11-09) | Non pour l'IPC et le Lua ; **oui pour la liste exacte** (0.57.2 rejetée) ; comportement layer, focus et activation modifié | **Critique** | https://github.com/hyprwm/Hyprland/releases/tag/v0.57.2 |
| Hyprland main | — | 70ec30e7 (2026-11-11) | `hyprctl keyword` retiré ; `version -j` perd les champs `*Hyprlang` | Moyenne (prochaine mineure) | https://github.com/hyprwm/Hyprland/commit/6170b35b |
| Rust | 1.97.0 | 1.99.1 (1.100.0 annoncée au 2026-11-12, NON VÉRIFIÉE sur le canal) | Non (même édition) | Moyenne (correctifs de miscompilation en 1.97.1 et 1.99.1) | https://static.rust-lang.org/dist/channel-rust-stable.toml |
| gpui-pre / -platform / -linux / -wgpu | 0.3.5 (zed@d89e9c2), vendorés et patchés | 0.3.12 (zed@d2a4fb6) | Oui (API pré-1.0 ; rebase des patches nécessaire ; conflit probable avec ext-background-effect) | Haute | https://crates.io/crates/gpui-pre |
| gpui (officiel) | non utilisé | 0.2.2 (2025-10-22) | — | Faible | https://crates.io/crates/gpui |
| blobin-ui-core | 0.1.0 @5c963b61 | NON VÉRIFIÉ (dépôt privé) | — | Moyenne | vendor/blobin-ui-core/VENDORED.md |
| zbus | 5.19.0 | 6.2.1 (dernière 5.x : 5.19.0) | **Oui** (API blocking supprimée, Value renommé Variant) | Haute | https://z-galaxy.github.io/zbus/upgrading-to-6.html |
| zvariant | 5.15.0 | 5.15.1 (fusionné dans zbus 6) | Non (5.15.1) | Moyenne (unmaintained) | https://rustsec.org/advisories/RUSTSEC-2026-0370 |
| ashpd | =0.13.13 | 0.14.0 (2026-11-05, zbus ^6) | Oui | Haute (couplé à zbus 6) | https://crates.io/crates/ashpd |
| notify-rust (transitive, gpui) | 4.18.0 (zbus ^5) | 4.19.0 (zbus ^6) | Semver-compatible, mais **ajoute zbus 6 en doublon** | Moyenne (piège de `cargo update`) | https://crates.io/crates/notify-rust |
| wayland-client | 0.31.15 | 0.31.15 | — | À jour | https://crates.io/crates/wayland-client |
| wayland-backend | 0.3.17 | 0.3.17 | — | À jour | https://crates.io/crates/wayland-backend |
| wayland-protocols | 0.32.13 | 0.32.13 | — | À jour | https://crates.io/crates/wayland-protocols |
| wayland-protocols-wlr | 0.3.12 | 0.3.12 | — | À jour | https://crates.io/crates/wayland-protocols-wlr |
| wayland-scanner | 0.31.11 | 0.31.11 | — | À jour | https://crates.io/crates/wayland-scanner |
| smithay-client-toolkit | 0.20.0 | 0.21.1 | Oui (0.x mineure) | Moyenne (gnos-ux-surface, gnos-lock, gnoshow) | https://crates.io/crates/smithay-client-toolkit |
| calloop | 0.14.4 | 0.14.5 | Non | Faible | https://crates.io/crates/calloop |
| calloop-wayland-source | 0.4.1 | 0.4.1 | — | À jour | https://crates.io/crates/calloop-wayland-source |
| pipewire / libspa | 0.10.1 | 0.10.1 | — | À jour | https://crates.io/crates/pipewire |
| serde | 1.0.229 | 1.0.229 | — | À jour | https://crates.io/crates/serde |
| serde_json | 1.0.151 | 1.0.154 | Non | Faible | https://crates.io/crates/serde_json |
| tokio | 1.53.2 | 1.54.1 (1.54.0 yanked) | Non | **Haute (RUSTSEC-2026-0386, unsound)** | https://rustsec.org/advisories/RUSTSEC-2026-0386 |
| rustix | 1.1.5 | 1.1.6 | Non | **Haute (RUSTSEC-2026-0486, unsound)** | https://rustsec.org/advisories/RUSTSEC-2026-0486 |
| nix | absent de Cargo.lock | — | — | — | — |
| image | =0.25.10 | 0.25.10 | — | À jour | https://crates.io/crates/image |
| tiny-skia | 0.11.4 | 0.12.0 | Oui | Faible (gnos-lock) | https://crates.io/crates/tiny-skia |
| resvg (transitive, gpui) | 0.46.0 | 0.50.0 | Oui (gpui 0.3.12 reste en 0.46) | Faible | https://crates.io/crates/resvg |
| fontdb (transitive) | 0.23.0 | 0.24.0 | Oui | Faible | https://crates.io/crates/fontdb |
| cosmic-text | 0.19.0 | 0.19.0 | — | À jour | https://crates.io/crates/cosmic-text |
| ttf-parser | =0.25.1 | 0.25.1 | — | Moyenne (unmaintained) | https://rustsec.org/advisories/RUSTSEC-2026-0192 |
| wgpu | 29.0.4 (gpui) + 30.0.1 (gnoshow) | 30.0.1 | — | Moyenne (doublon ; disparaît avec gpui-pre 0.3.12) | https://crates.io/crates/wgpu |
| uuid | 1.26.1 | 1.29.2 | Non | Faible | https://crates.io/crates/uuid |
| sha2 | 0.10.9 | 0.11.1 | Oui | Faible | https://crates.io/crates/sha2 |
| signal-hook | 0.3.18 | 0.4.6 | Oui | Faible | https://crates.io/crates/signal-hook |
| thiserror | 1.0.69 + 2.0.20 | 2.0.21 | 1 → 2 rupture | Faible (unifier sur 2) | https://crates.io/crates/thiserror |
| toml | 0.8.23 (gnos-modules) | 1.2.1 | Oui | Faible | https://crates.io/crates/toml |
| futures | 0.3.34 | 0.3.34 | — | À jour | https://crates.io/crates/futures |
| futures-lite | 2.6.1 | 2.6.1 | — | À jour | https://crates.io/crates/futures-lite |
| anyhow | 1.0.104 | 1.0.104 | — | À jour | https://crates.io/crates/anyhow |
| chrono | 0.4.45 | 0.4.45 | — | À jour | https://crates.io/crates/chrono |
| libc | 0.2.189 | 0.2.191 (1.0.0-alpha.5 existe) | Non | Faible | https://crates.io/crates/libc |
| memmap2 | 0.9.11 | 0.9.12 | Non | Faible | https://crates.io/crates/memmap2 |
| pam-client | 0.5.0 | 0.5.0 (2022) | — | Faible (crate peu active) | https://crates.io/crates/pam-client |
| unicode-segmentation | 1.13.3 | 1.13.3 | — | À jour | https://crates.io/crates/unicode-segmentation |
| bytemuck | 1.25.2 | 1.25.2 | — | À jour | https://crates.io/crates/bytemuck |
| pollster | 0.4.0 (+0.2.5) | 1.0.1 | Oui | Faible | https://crates.io/crates/pollster |
| raw-window-handle | 0.6.2 | 0.6.2 | — | À jour | https://crates.io/crates/raw-window-handle |

## Recommandations (avis, pas des faits)

1. **Hyprland : qualifier 0.57.2 avant que le poste la reçoive.** Il faut la passer comme 0.57.1 sur le banc (focus pointeur au map d'une layer, layers bottom et background sous fullscreen, xdg-activation sans serial, verrouillage), puis l'ajouter à `SUPPORTED_HYPRLAND_VERSIONS` comme candidate. Garder la liste exacte, c'est le choix de conception ; mais sans qualification préalable, une mise à jour Nix coupera les chemins d'activation.
2. **Hyprland : actualiser les commentaires et les parseurs marqués « Hyprland 0.56 »** (`keyboard_lock.rs`, `gnos-thumbnails`, `scene_runtime.rs`) pour qu'ils citent la version réellement qualifiée. Surveiller la future 0.58 (`keyword` retiré, champs de `version -j`) : gnosix n'en dépend pas aujourd'hui, mais un test de non-régression sur `hyprctl -j version` sans `buildHyprlang` est peu coûteux.
3. **Patches sûrs maintenant, sans rupture** : mettre à jour tokio (≥1.53.3, idéalement 1.54.1) et rustix (≥1.1.6) dans le lockfile, avec un `cargo update -p tokio -p rustix` ciblé et **pas** un `cargo update` global. Cela clôt les deux avis « unsound ». Éviter `notify-rust 4.19.0` (le garder en 4.18.0 via `cargo update -p … --precise`, ou ne pas le toucher) tant que la migration zbus 6 n'est pas décidée.
4. **zbus 6 : lot dédié, pas maintenant.** Il faut réécrire les 6 crates qui utilisent `zbus::blocking` et faire le renommage `Value` en `Variant`. Coordonner avec ashpd 0.14 et avec un gpui-pre qui adopterait ashpd 0.14 (pas encore le cas en 0.3.12, NON VÉRIFIÉ pour les snapshots à venir).
5. **GPUI : planifier un rebase 0.3.5 → 0.3.12** sur le seam blur. L'amont a désormais ext-background-effect ; il faudrait réduire le patch gnosix à « régions typées » au-dessus de l'implémentation amont. Gains : wgpu unique (30), améliorations d'activation. Les patches sur la taille layer-shell 0, `set_keyboard_interactivity` et les curseurs restent nécessaires et seraient candidats à un envoi amont.
6. **Toolchain** : la règle « pin = toolchain qualifiée » est respectée. Au prochain passage de la matrice, viser au minimum 1.97.1 (correctif de miscompilation LLVM). Mieux, viser 1.99.1, qui corrige aussi les retours `bool` FFI sur x86_64, ce qui est pertinent vu l'usage de libc et PAM.
7. **Faible priorité** : unifier thiserror sur 2, smithay-client-toolkit 0.21, sha2 0.11, signal-hook 0.4, toml 1.x ; remplacer à terme ttf-parser, qui est unmaintained, par skrifa.

## Limites
- L'API GitHub (releases Hyprland) était refusée dans cette session : le texte des notes de version n'a pas été lu. Les changements viennent du `git log` et du `git diff` entre les tags.
- `cargo update` et `cargo audit` n'ont tourné que dans la copie du scratchpad. Le Cargo.lock du dépôt n'a pas été touché.
- Pas de build ni de tests sur les versions mises à jour : leur compatibilité de compilation est NON VÉRIFIÉE.

— audit réalisé par un sous-agent de Syster Agent
