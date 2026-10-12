# G07B-02J — Feuille E4 perceptuelle (non ratifiée)

**État : PENDING HUMAN REVIEW.** Ce document est un protocole de revue, pas une attestation.

## Environnement obligatoire

Utiliser un banc Wayland isolé, explicitement identifié par commit, SHA du binaire, version de Hyprland/Aquamarine et deux sorties réellement actives. Ne jamais modifier automatiquement le Hyprland principal. Capturer la position des fenêtres, la géométrie des moniteurs, l'état du pointeur et l'horodatage des événements du compositeur.

## Parcours à examiner

1. **DnD local / WAYLAND-1 et WAYLAND-2.** Suivre une vraie source vers une vraie cible, sans interpolation artificielle. Exiger `wl_data_device.enter`, `drop`, checksum payload et `dnd_finished`.
2. **DnD inter-sorties dans les deux directions.** Enregistrer le pointeur avant, pendant et après le bouton maintenu. Vérifier visuellement la continuité aux frontières et la cible réelle, et confronter au `wl_data_device.enter` de la surface destinataire.
3. **Lock initial et lock recréé.** Détruire/recréer sous focus contrôlé, tester mouvements courts et longs, les événements relatifs et les coordonnées locales au sein de la surface. Un seul `locked` ne certifie pas la stabilité.
4. **Variations du focus et déconnexions.** Répéter après changement de fenêtre et de sortie hôte, remapping, fermeture de la cible et déplacement du curseur hors fenêtre.
5. **Rendu perçu.** Examiner animations, tearing, déplacement non voulu du pointeur, surfaces fantômes et transitions multi-écrans à l'écran, idéalement sur vidéo horodatée.

## Attestation requise

Documenter chaque parcours : environnement identifié, date, opérateur, captures ou vidéo, résultat PASS/FAIL/BLOCKED, anomalies et références d'événements. Pour chaque FAIL, garder l'artefact brut. Pour les tests non exécutés, inscrire `NOT_RUN`, sans convertir l'absence d'erreur en PASS.

**Qualification actuelle : aucune revue humaine E4 ratifiée. G07-B production reste BLOCKED.**
