# G07B-02J — Reproduction contrôlée

## Audit hors runtime (préférable)

Depuis le dépôt Gnosix canonique :

```sh
python3 docs/evidence/g07b02j/independent_audit.py
python3 docs/evidence/g07b02j/offline_regressions.py
python3 docs/evidence/g07b02j/make_receipt.py
```

Les scripts vérifient directement les archives `traces/*.zst` et les reçus historiques sous `../g07b02g2`, `../g07b02h`, `../g07b02i`. L'audit n'a besoin ni de compositor actif ni de hardware GPU. Il doit retrouver **38 motions hors de la première fenêtre**, **0 enter/leave pendant le grab**, **quatre contrôles lock stables** et **19 fichiers historiques vérifiés**.

## Reprise live conditionnelle

1. Créer un **worktree temporaire** depuis `main`; ne jamais éditer directement un artefact gelé.
2. Utiliser un compositeur de test privé, version/source et SHA documentés. La session principale doit rester intacte.
3. Construire les clients natifs via `../g07b02i/build_clients.sh <répertoire temporaire>`.
4. Renseigner `G07B02I_PARENT_SIG`, `G07B02I_NESTED_SIG`, `G07B02I_NESTED_PID`, `G07B02I_SOCKET`, `G07B02I_RUN_ROOT`, `G07B02I_TARGET_OUTPUT` pour `lock_repro.py` ; ajouter `G07B02J_DELTA` et `G07B02J_STEPS`. Pour une recréation, utiliser `G07B02I_RECYCLE=1`.
5. Capturer le parent avec `WAYLAND_DEBUG=client`, et toutes les observations sans rediriger vers la session de production. Restaurer le curseur, fermer l'instance et conserver la trace des processus arrêtés.

**Ne pas extrapoler :** un PASS documentaire ne remplace pas un E2E, et le test Weston bi-output ne prouve pas le DnD indépendant ou la relative motion physique.
