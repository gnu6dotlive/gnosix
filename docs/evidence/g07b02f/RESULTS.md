# G07B-02F — Multi-Output & Boundary Race Qualification

**12 octobre 2026 · qualification ciblée PASS · promotion G07-B BLOCKED**

## But et protocole

Confronter la correction candidate de la réévaluation du focus lors de commits `wl_surface.input_region` à deux emplacements physiques distincts, avec animation GPUI `SharedMotion` pilotée par `on_next_frame` et timer, puis déterminer la destination correcte des clics aux frontières mobiles par des événements **internes au compositeur**.

Version source du compositeur expérimental : Hyprland `v0.56.2`, commit `efb50993780079460b0cbed1363e2166a2de1d9f`. L'instrumentation G07B-02E `5a4b7e6` et le patch de focus déjà qualifié sont utilisés sans nouvelle modification produit. Gnosix evidence baseline : `4f086c4`. Binaire expérimental et binaire principal hashés dans `receipt.json`.

### Point essentiel : portée « deux sorties »

Les tests ont été réalisés **séquentiellement** : une instance Hyprland corrigée, imbriquée dans une fenêtre flottante 1250 × 800 sur la sortie physique DP-2; puis une nouvelle instance de même binaire placée sur HDMI-A-1. Les deux fenêtres étaient ancrées et leur centre associé explicitement aux coordonnées des moniteurs du compositeur principal :

| Sortie hôte | Géométrie physique | Position de fenêtre imbriquée |
|---|---|---|
| DP-2 | (1930,390), 1920 × 1080 | (2120,515), 1250 × 800 |
| HDMI-A-1 | (-630,590), 2560 × 1440 | (40,795), 1250 × 800 |

Dans l'instance imbriquée, les deux processus GPUI distincts (overlay et witness) utilisent des surfaces layer-shell 900 × 560 exactement superposées. Les tests sont donc représentatifs des **emplacements physiques**, mais **ne représentent pas une instance Hyprland patchée contrôlant nativement les deux têtes simultanément**.

## Matrice de commits déterministes

La même matrice de 16 étapes sur les deux emplacements (ouverture, retrait, fractions intermédiaires, trous et marges non interactives, répétition de cycles) :

| Hôte | Clics corrects | Changements d'appartenance |
|---|---:|---:|
| DP-2 | 16/16 | 10 |
| HDMI-A-1 | 16/16 | 10 |
| **Total** | **32/32** | **20** |

Les commits sont confirmés par `G07B02C_APPLY` et `G07B_REGION`; la position du curseur est contrôlée avant injection. Le harnais vérifie que le parent reste flottant, que la géométrie ne change pas et que la fenêtre est réellement sur la sortie physique attendue.

## Matrice temporelle réelle — qualification par époques du compositeur

Quatre animations GPUI complètes, chacune avec 21 événements `MATCHED_TARGET` et son `MATCHED_COMPLETE` :

| Hôte | Politique | Callbacks GPUI | Boutons attribués | Régions correctes à l'appui | Transitions pendant press–release |
|---|---|---:|---:|---:|---:|
| DP-2 | `on_next_frame` | 249 | 34/34 | 34/34 | 4 |
| DP-2 | timer 13 ms | 590 | 34/34 | 34/34 | 2 |
| HDMI-A-1 | `on_next_frame` | 249 | 34/34 | 34/34 | 6 |
| HDMI-A-1 | timer 13 ms | 657 | 34/34 | 34/34 | 4 |
| **Total** | | | **136/136** | **136/136** | **16** |

**Oracle indépendant** : `g07b02f_epoch_audit.py` extrait les époques `G07B02E_REGION`, `FOCUS`, `BUTTON` du compositeur, associe les identités internes de focus aux deux clients via les transitions de région, identifie un seul appui et un seul relâchement à chaque injection, et confronte la destination `G07B_OVERLAY_CLICK` ou `G07B_WITNESS_CLICK` horodatée à l'identité du focus **et à l'appartenance de la région appliquée à l'appui**. Les 136 couples appui/destination sont qualifiables et cohérents. Le dispositif ne s'appuie donc pas sur la fraction GPUI observée *après* l'événement pour deviner l'ancienne destination.

### Trois anomalies préliminaires expliquées

Le classificateur original, basé sur les rectangles client au début et à la fin d'une injection, signalait un clic incorrect dans chacun de `dp2-frame`, `hdmi-frame` et `hdmi-timer`. Le cas DP-2 montre :

- Le compositeur livre `BUTTON state=1` alors que le focus appartient encore à l'overlay.
- Le nouveau `REGION old=true new=false` arrive **après** l'appui.
- `FOCUS` est ensuite réévalué vers le témoin, puis le relâchement survient.
- Le callback overlay enregistre `inside=false` parce que sa fraction visuelle a déjà diminué.

Cela correspond à une capture implicite normale de la séquence initiée sur l'overlay, non à une preuve de mauvaise livraison. L'auditeur indépendant vérifie aussi les deux autres cas; les **trois** sont cohérents avec la région appliquée **au moment du press**. Aucun de ces trois cas n'est revendiqué comme un clic dans une région inchangée.

## Livraison et intégrité

- `receipt.json` — identités exactes et hachages, six résultats, gates.
- `button_epoch_audit.json` — 136 événements avec époques de press et release, destinataires, régions appliquées et contrôles de cohérence.
- `g07b02f_epoch_audit.py` — auditeur indépendant reproductible.
- `g07b02f_output_stress.py`, `g07b02f_output_motion.py` — variantes à préflight explicite de sortie physique.
- `traces/*.tar.zst` — six archives complètes de clients GPUI et témoins; `traces/{dp2,hdmi}-compositor.log.zst` — deux journaux du compositeur; **8/8 archives vérifiées par `zstd -t`**.

## Non-claims / verdict

**PASS borné** pour les deux emplacements physiques distincts, la matrice déterministe 32/32, les quatre animations terminées et les 136 événements dont la destination respecte l'état de focus/région appliqué côté compositeur.

**Non qualifiés** : ancienne matrice native sur **une seule instance patchée à deux sorties**, surfaces layer-shell directement publiées sur les deux sorties par cette instance, confinement/verrouillage explicite du pointeur, DnD matériel, E4 perceptuel et stress prolongé. Les valeurs de callbacks ne prouvent aucune supériorité de performance. La sécurité/ergonomie d'un déploiement du patch Hyprland au bureau principal n'est pas démontrée.

Aucun merge, push ni modification de `/usr/bin/Hyprland`; les instances de compositeur et les clients expérimentaux ont été arrêtés. **G07-B production : BLOCKED.**

**Suite : G07B-02G — Native Two-Head & Constraint Gate.** Reproduire la matrice d'origine à l'intérieur d'une *unique instance patchée multi-output* avec deux sorties Wayland effectives et coordonnées indépendantes, puis ajouter `zwp_pointer_constraints_v1`, capture DnD et qualification visuelle E4. Ne pas substituer la session utilisateur non protégée à un banc isolé.
