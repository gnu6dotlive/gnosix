# Consolidation des worktrees — 12 octobre 2026

**Statut : intégration documentaire locale uniquement. Aucun déploiement ni promotion du code expérimental.**

## Règle canonique

- La branche `main` est la destination locale des **preuves, décisions, statuts et références historiques** qualifiées pour publication ultérieure après vérification éditoriale. Aucun exécutable G07B ni changement de dépendance n'a été transféré par cette opération.
- Les développements produits qui ne satisfont pas leurs gates restent sur leurs branches `feat/*` existantes, **sans worktree résident**. Les branches `probe/*` ne constituent **jamais** une destination durable.
- Chaque tête Git de l'inventaire initial est préservée localement sous `refs/archive/worktrees/2026-10-12/<nom>-<sha10>`. Les références d'archive sont un mécanisme de récupération, **pas une validation de production ni une publication distante**.
- Le dépôt Hyprland expérimental conserve séparément sa tête `4feb578` sous `refs/archive/worktrees/2026-10-12/hyprland-g07b02g1-4feb578`; son commit reste une proposition non intégrée à Hyprland système.

## Qualification conservée

| Programme | Preuve disponible | Limite non franchie |
|---|---|---|
| Cabinet / Files / Fluid F12C | Rapports `docs/status/CABINET_*` historiques et têtes expérimentales archivées | Aucune intégration de l'exécutable Cabinet dans `main` par cette consolidation |
| G07-A — Click-through statique | Qualifications historiques dans `docs/status` | Ne prouve pas le routage dynamique ni les contraintes explicites |
| G07B-02G1 — Deux sorties Wayland imbriquées simultanées | 32/32 clics déterministes; 134/134 événements temporels totalement attribuables corrects | 1 injection de 135 avec callback non attribué; DnD, confinement, E4 et production restent ouverts |

Le contenu de `docs/status` et `docs/evidence` contient plusieurs rapports de **moments historiques différents**, y compris des échecs conservés. Leur présence dans `main` ne change pas les verdicts de ces rapports et **ne signifie pas** que le code décrit a été fusionné au produit.

## Discipline de reprise

Créer un worktree **temporaire** depuis `main` ou la branche produit appropriée, avec un contrat et un propriétaire précis. Après qualification : transférer les résultats autorisés vers la destination canonique, créer une référence d'archive pour les négatifs et propositions, vérifier `git status`, puis supprimer le worktree et la branche `probe/*`. Ne pas transformer le répertoire d'un essai en espace de développement permanent.

Le relevé complet des SHA, branches, chemins locaux et éventuelles pièces ignorées par Git est conservé **hors du dépôt public potentiel** dans `/mnt/workbench/capsule-donor-01/WORKTREES-ARCHIVE-MANIFEST-20261012.json`. Le classement précis des archives locales n'est pas publié automatiquement.

**Interdit sans qualification supplémentaire :** intégrer le patch Hyprland G07B dans l'exécutable système, déclarer G07B production PASS, effacer les preuves négatives ou pousser les références d'archive vers une forge publique.

## Clôture effective — audit après opérations

- **Worktrees Gnosix : 31 → 1**. Le seul worktree enregistré est `/home/tension_atoi/Projects/gnosix` sur `main`.
- **Têtes initiales archivées : 31/31**, résolues aux SHA initiaux. Les **21 branches locales `probe/*`** ont été fermées, plus 2 branches locales déjà fusionnées. Les 8 branches `feat/*` en attente restent des références de développement produit **sans worktree associé**, et leur intégration n'est pas prétendue.
- Hyprland source expérimental : checkout détaché sur le commit de référence `efb5099`; la branche `probe/g07b02g1-output-local-abs-warp-20261012` a été fermée, son correctif `4feb578` préservé sous la référence `refs/archive/worktrees/2026-10-12/hyprland-g07b02g1-4feb578`.
- **Sources ignorées par Git** : cinq archives locales vérifiées (`ignored-worktree-sources-20261012/`) pour les copies GPUI expérimentales, avant retrait des répertoires d'essai.
- **Données non suivies protégées** : les 51 fichiers de `docs/diagrams/` du répertoire principal n'ont pas été ajoutés, modifiés ou supprimés.
- Le commit documentaire canonique d'import est `76b408f`, descendant de `origin/main` local tel que présent à l'inventaire. Les preuves de G07B restent en statut bloqué. Aucun push, aucun remplacement du compositeur et aucune validation de production par la présente clôture.
