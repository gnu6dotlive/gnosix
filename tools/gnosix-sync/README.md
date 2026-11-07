# gnosix-sync — canal de mise à jour locale

Le canal entre l'agent (qui travaille dans un environnement virtuel, sans
session Wayland ni Hyprland) et le poste de l'opérateur (seul endroit où Gnosix
tourne vraiment). GitHub sert uniquement à partager les branches de travail :
**aucune GitHub Action n'est utilisée**. Compilation, tests et redémarrage se
font en local.

## Le cycle

1. L'agent pousse une branche et donne **une commande exacte**, par exemple :

   ```bash
   gnosix-sync pull claude/lucid-bell-igrae7 --expect 1a2b3c4
   ```

   `--expect` épingle le commit poussé : si GitHub ne le montre pas encore,
   ou si la branche a bougé, rien n'est fait.

2. Sur le poste, `gnosix-sync` :
   - récupère la branche dans le dépôt local (`~/Projects/gnosix` par défaut) ;
   - la place dans un worktree détaché (`.worktrees/gnosix-sync`), sans jamais
     toucher la copie de travail de l'opérateur ;
   - adopte la version de `gnosix-sync` portée par la branche si elle diffère ;
   - compile les binaires déclarés dans `tools/gnosix-sync/plan` ;
   - lance les vérifications déclarées par ce même plan ; le premier échec
     arrête tout et **rien n'est déployé** ;
   - arrête le service en cours (`systemctl --user stop`), installe les
     nouveaux binaires dans `/usr/libexec/gnosix` (l'ancien est gardé en
     `<nom>.prev`), redémarre, puis surveille le service quelques secondes ;
   - si le nouveau build ne tient pas (service inactif ou redémarré par
     systemd), il **revient automatiquement** à l'ancien binaire.

3. En cas d'erreur, l'opérateur envoie le rapport à l'agent :

   ```bash
   gnosix-sync report
   ```

   Le rapport (`REPORT.md`) contient la branche, le commit, l'étape en échec,
   les lignes d'erreur extraites et la fin du journal (cargo, tests ou
   `journalctl` du service). Il est copié dans le presse-papiers si `wl-copy`
   est présent.

## Première installation (une seule fois)

```bash
git -C ~/Projects/gnosix fetch origin claude/lucid-bell-igrae7
```

```bash
git -C ~/Projects/gnosix show FETCH_HEAD:tools/gnosix-sync/gnosix-sync > /tmp/gnosix-sync && bash /tmp/gnosix-sync install
```

Ensuite, l'outil se met à jour tout seul depuis les branches qu'il récupère.

## Commandes

| Commande | Effet |
| --- | --- |
| `gnosix-sync pull <branche> [--expect <sha>]` | cycle complet |
| `… --no-deploy` | compile et teste sans toucher au service |
| `… --no-checks` | compile et déploie sans les vérifications |
| `… --check "<commande>"` | ajoute une vérification ponctuelle |
| `gnosix-sync report` | affiche/copie le dernier rapport |
| `gnosix-sync status` | build déployé, état des unités, dernier run |
| `gnosix-sync rollback` | restaure les binaires `.prev` et redémarre |

## Le plan (`tools/gnosix-sync/plan`)

Chaque branche porte son propre plan ; l'agent le met à jour avec le travail.

```text
check  <commande shell, lancée à la racine du worktree>
deploy <crate> <binaire cargo> <nom dans /usr/libexec/gnosix> <unité systemd --user | -> [features]
```

## Configuration

Facultative, dans `~/.config/gnosix-sync/config` (fichier bash) :

```bash
GNOSIX_SYNC_REPO="$HOME/Projects/gnosix"        # dépôt local
GNOSIX_SYNC_INSTALL_DIR="/usr/libexec/gnosix"   # cible des binaires (sudo si non inscriptible)
GNOSIX_SYNC_HEALTH_SECONDS=6                    # durée de surveillance après démarrage
```

Les journaux de chaque run sont dans `~/.local/state/gnosix-sync/runs/`.

## Limites

- L'écriture dans `/usr/libexec/gnosix` demande `sudo` : c'est une mutation de
  l'hôte, autorisée par l'opérateur au moment où il lance la commande.
- La surveillance détecte un service qui tombe ou redémarre ; elle ne juge pas
  le rendu. La validation visuelle reste celle de l'opérateur.

— Syster Agent
