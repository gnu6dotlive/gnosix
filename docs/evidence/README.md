# Evidence model

gnosix treats evidence as support for a bounded claim, not as decoration and not as a substitute for product documentation.

## Evidence levels

- **Source qualification** — deterministic tests, type/API boundaries, compile-fail checks, source invariants, and repository-hermetic gates.
- **Runtime observation** — a behavior was observed in a named runtime/fixture with retained provenance.
- **Installed-path qualification** — the exact installed artifact/path and real service/caller boundary were exercised.
- **Operator review** — a human/perceptual/product judgment was explicitly recorded where automation cannot establish the claim.

Higher-looking evidence does not automatically widen a claim. An installed-path proof for one operation does not certify every operation of the product.

## Public evidence policy

Public docs link the smallest evidence set needed to support current claims. Historical failures and counterevidence are retained rather than rewritten into success, but the raw engineering corpus is not the navigation model for the public project.

The current private engineering tree still contains a large raw `evidence/` corpus. Publication work will preserve that provenance while selecting only claim-bearing receipts/captures for the curated public cut.

## Current public ledger

The public repository carries a curated claim ledger rather than the raw workstation receipt corpus. See [CURRENT.md](CURRENT.md) for current claim boundaries, inspectable source references, retained private provenance digests, and explicit limitations.

The private engineering corpus remains the provenance archive. Promoting a raw receipt or capture into the public repository requires a separate privacy and usefulness review; publication is never implied by the existence of an internal artifact.

See [current status](../status/STATUS.md) for the product claim associated with each evidence record.
