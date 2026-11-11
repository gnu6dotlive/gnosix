# Registre terrain — 2026-11-11

Source : retour de l'opérateur et rapport `TERRAIN — 2026-11-11 —
claude/lucid-bell-igrae7@72b5b03` de l'agent de terrain. Chaque entrée sépare
**faits** (observés sur le poste ou lus dans le source), **diagnostic** (cause
établie ou hypothèse, dite comme telle) et **direction** (ce qu'on propose).

Machine de référence : Hyprland 0.57.1 (Nix, `acc1aa0`) ; HDMI-A-1 = LG QHD
2560×1440@74.96 *ou* téléviseur Samsung selon le câble ; DP-2 = Acer ED320QR S
1920×1080@164.92, portrait (transform 1). Build en service : `~/.local/lib/gnosix/gnos-ux-scene`
du 2026-10-31, sans les features `r12/r13/indicator-production-client`.

Priorités : **P0** débloque le poste ; **P1** comportement attendu au quotidien ;
**P2** refonte ou nouvelle fonction.

---

## H — Intégration Hyprland

### H1 · L'autorité d'activation refuse Hyprland 0.57.1 — P0 · corrigé dans ce commit

- **Faits.** `in-gnu-os-activation` est en `start-limit-hit` depuis le
  2026-11-09 13:55, statut « Starting: waiting for Hyprland observation ».
  Le source comparait la version par égalité stricte avec
  `SUPPORTED_HYPRLAND_VERSION = "0.56.2"` (`gnosix-hyprland/src/lib.rs`,
  `activation_execution.rs`, `production_host.rs` — ce dernier par simple
  `contains`, qui aurait aussi accepté `0.56.20`).
- **Diagnostic.** Établi par le source : toute version ≠ 0.56.2 est refusée.
- **Fait.** Liste exacte `SUPPORTED_HYPRLAND_VERSIONS = ["0.56.2", "0.57.1"]` ;
  correspondance par jeton entier ; 0.57.1 admise comme **candidate** (sa
  qualification est le passage terrain). Tests : 60/60 `gnosix-hyprland`,
  clippy propre avec `r13-production-dbus`.
- **Reste.** Le service d'activation lance `/usr/libexec/gnosix/in-gnu-os-activationd`
  (pas de surcharge utilisateur). `gnosix-sync` installe le nouveau binaire dans
  `~/.local/lib/gnosix/` sans toucher au service : décision opérateur (Q6).
  Note : ce binaire différera du digest figé `tools/r13-authority.sha256`.

### H2 · Stratégie d'intégration Hyprland — P1

- **Faits.** Huit points lancent `hyprctl` directement, répartis sur quatre
  crates : `compositor_blur.rs` (`eval`), `scene_runtime/bar_keys.rs`
  (`-j monitors`), `hyprland_live.rs` (`repl`), `gnosix-hyprland/src/lib.rs`
  (`repl`/`eval`), `production_host.rs` (`version`), `gnos-lockd.rs`
  (`dispatch` dpms), `gnos-utilities/src/displays.rs` (`eval`, `-j monitors all`).
  Plusieurs commentaires et parseurs disent « Hyprland 0.56 ».
- **Diagnostic.** Chaque mise à jour de Hyprland casse un sous-ensemble
  imprévisible ; aucun point unique ne dit ce qui est compatible.
- **Direction.**
  1. Un seul adaptateur (`gnosix-hyprland`) possède le transport (socket IPC
     `.socket.sock`/`.socket2.sock`, déjà présent pour l'observation) ; les
     autres crates consomment des modèles typés, jamais `hyprctl`.
  2. Compatibilité **par capacité** sondée au démarrage (la fonction Lua existe,
     le champ JSON est là), pas par égalité de version ; la liste de versions
     ne sert plus qu'à étiqueter le niveau de preuve.
  3. Fixtures capturées par l'agent de terrain pour chaque version (sorties
     `-j monitors`, `layers`, `devices`, réponses Lua) et tests de contrat dessus.

### H3 · `gnusnip` cassé — P1 · données manquantes

- **Faits.** `gnusnip` n'est pas dans ce dépôt ; seule sa liaison clavier
  (`~/.config/hypr/gnusnip.lua`) est lue par `xtask/src/keybindings.rs`.
- **Manque.** Où est son code, quel symptôme exact, quel message d'erreur (Q5).

### H4 · Curseur dédoublé dans le tiers droit du LG — P0 · cause établie

- **Faits.** `~/.config/hypr/hyprland.lua:45-46` déclare HDMI-A-1 en
  1920×1080@60 en 10,590 et DP-2 en 1930,390. Le LG tourne réellement en
  2560×1440 : il couvre x 10→2570. DP-2 commence à x 1930 : **640 px de
  chevauchement** (x 1930→2570, y 590→2030). C'est exactement la bande droite
  du LG où le curseur apparaît sur les deux écrans.
- **Correctif immédiat (configuration de l'opérateur, pas Gnosix).** Placer
  DP-2 à x = 2570 (10 + 2560), ou mieux, décrire les écrans par identité
  (H5). Le Deck et le rail sont à re-mesurer après (D4).

### H5 · L'écran est identifié par son port, pas par l'appareil — P1

- **Faits.** Le même câble HDMI porte tantôt le LG, tantôt le téléviseur
  Samsung. La scène route par nom de connecteur (`GNOS_UX_TOPOLOGY_ROUTE …
  HDMI-A-1`). Pourtant `in-gnu-os-output-identity` modélise déjà des empreintes
  EDID/numéro de série (statut public : SOURCE-QUALIFIED).
- **Direction.** Tous les réglages par écran (bord du Bar, échelle, position,
  Deck) se rattachent à l'**identité logique** (empreinte EDID), le connecteur
  n'étant qu'un attachement de session. Un appareil inconnu reçoit des valeurs
  par défaut sûres, jamais celles de l'appareil précédent sur ce port. Côté
  Hyprland, les règles d'écran utilisent `desc:` plutôt que le connecteur.
  Gnosix ne doit plus supposer une résolution fixe.

---

## D — Deck et AppStack

### D1 · AppStack ne voit pas les applications installées — P1

- **Faits.** `gnosix-application-catalog` lit les répertoires XDG
  (`XDG_DATA_DIRS`) au chargement ; aucune surveillance ensuite. D'où le
  redémarrage nécessaire après l'installation d'un jeu Steam.
- **Direction.** Surveillance **par événements** (inotify) des répertoires
  `applications/` (utilisateur, système, Flatpak, Steam qui écrit dans
  `~/.local/share/applications`), regroupée sur ~500 ms, puis recalcul du
  catalogue. Aucun sondage périodique. Plus tard, un service « catalogue »
  commun pourrait aussi servir les mises à jour logicielles (à cerner).

### D2 · Bouton AppStack ↔ barre de recherche : rupture visuelle — P2 · design

- **Faits (opérateur).** Le bouton est primitif ; au clic, la barre de
  recherche apparaît dans un autre style. La barre elle-même est correcte.
- **Direction.** Un seul objet qui se transforme (le bouton *devient* le
  champ). Il faut une référence visuelle de l'opérateur (Q3).

### D3 · Curseur de saisie décoratif — P1 · cause établie

- **Faits.** Le curseur (caret) ne clignote pas ; ← et → ne le déplacent pas.
  Le source montre pourquoi : en mode grille, `set_grid_arrows` (DECK-01C)
  envoie ← → aux tuiles et le champ les laisse passer
  (`product_appstack_search_input.rs`).
- **Direction.** ← → éditent le texte tant que le champ a le focus ; les
  tuiles se parcourent avec ↑ ↓ et Tab. Caret qui clignote (fixe si
  « mouvement réduit »), sélection Maj+flèches, Ctrl+← → par mot.

### D4 · Topologie du Deck et du rail — P1 · à re-mesurer

- **Faits (terrain).** Bords incohérents de 10 px (DP-2 : gauche h=1866 vs
  droite h=1876 ; bas w=1070 vs haut w=1080) ; 288 réconciliations
  topologiques en 30 min de session active.
- **Diagnostic.** Probablement aggravé par H4. La scène fait confiance à la
  géométrie de Hyprland et ne détecte pas les chevauchements (à vérifier).
- **Direction.** Après H4 : re-mesure ; puis détection explicite des
  chevauchements (avertissement journalisé, pas de rejet) et test de
  contrat sur la symétrie des bords du rail.

---

## B — Bar

### B1 · Ouvrir « Session » prend le focus — P1 · design à préciser

- **Faits (opérateur).** Avec le menu Session ouvert sur un écran, cet écran
  perd la réaction au survol (forme du curseur), l'autre la garde. Clic
  ailleurs = fermeture, ce qui est voulu.
- **Source.** La scène est en `KeyboardInteractivity::OnDemand` : un clic sur
  la couche lui donne le focus clavier.
- **Direction.** L'ouverture est une **bascule**, pas une prise de focus ;
  même comportement sur tous les écrans. Le détail dépend de Q1.

### B2 · Tiroir pour les icônes de la zone de notification — P2

- **Faits.** Le panneau « … » (BAR-02A) ne reçoit que ce qui ne tient pas
  sur le bord ; on ne peut pas choisir de ranger une icône.
- **Direction.** Un bouton tiroir ; clic droit sur une icône → « Ranger » ;
  glisser-déposer entre le Bar et le tiroir ; choix enregistré dans le profil.

### B3 · Toutes les icônes d'état ouvrent Quick Toggles — P1 · spec UX d'abord

- **Faits.** Le groupe d'icônes d'état est **une seule cible**
  (`IdleBarEntry::StatusIcons` → `toggle_utilities`, SURF-01B). Des popouts
  par icône existent déjà (réseau, Bluetooth, batterie) mais ne sont pas
  routés par le clic.
- **Direction.** Chaque icône ouvre et ferme **son** panneau (volume : sortie,
  périphérique, niveau ; réseau ; Bluetooth). Quick Toggles est démonté et
  ses bascules réparties. Spécification UX à écrire et valider avant le code (Q2).

---

## K — Handy

- **Faits.** `handy.service` est inactif depuis le 2026-11-09 17:07:58,
  sorti avec le code 0 : `Restart=on-failure` ne le relance donc pas. Le
  publisher est bien compilé dans Handy (`~/Projects/handy` @ `e9b92c2`). La
  scène en service n'a pas le client indicateur.
- **Direction.** Déployer la scène avec la feature (déjà au plan) ; démarrer
  Handy ; comprendre pourquoi il sort proprement (journal du 09/11 17:07) ;
  puis observer l'échange D-Bus pendant une dictée.

---

## Réponses aux questions de l'agent de terrain

1. **Chevauchement des sorties.** La scène fait confiance à Hyprland ; aucun
   rejet n'est prévu dans `rail_topology`/`deck_dock`. Proposé en D4 :
   détecter et journaliser, ne pas rejeter.
2. **Autorité d'activation absente.** Le client d'activation est appelé par
   requête (activation de bureau) et rend un refus typé : la scène devrait
   rester debout, seules les activations échouent. **Hypothèse à vérifier**
   dans le journal après déploiement. La cause du `start-limit-hit` est H1.

## Questions ouvertes pour l'opérateur

- **Q1 (B1).** Menu du Bar ouvert : les fenêtres de **tous** les écrans
  arrêtent-elles de réagir à la souris (un clic ailleurs ne fait que fermer
  le menu) ? Ou les fenêtres continuent-elles à réagir normalement, et seul
  le focus ne doit pas bouger ?
- **Q2 (B3).** Où vont les bascules de Quick Toggles (ne pas déranger,
  rester éveillé, micro) ? Chacune dans le panneau de son icône, ou un petit
  panneau « système » ?
- **Q3 (D2).** Une référence visuelle (capture, croquis, application) pour le
  bouton AppStack ?
- **Q4 (H5).** Quand le téléviseur est branché : étendre ou dupliquer ? Le
  Bar s'y affiche-t-il ?
- **Q5 (H3).** Où est le code de `gnusnip`, et que se passe-t-il exactement ?
- **Q6 (H1).** Pour le démon d'activation : surcharge systemd utilisateur vers
  `~/.local/lib/gnosix/` (comme la scène et les profils), installation dans
  `/usr/libexec` avec `sudo`, ou on attend ?

— Syster Agent
