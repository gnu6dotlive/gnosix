# Third-party notices

Unless a file or directory states otherwise, first-party Gnosix code and repository documentation are licensed under the **MIT License**. See [`LICENSE`](LICENSE). This supersedes the prior first-party GPL policy for contributions and distributions from the MIT-licensed source revision onward; it does not revoke rights previously granted under GPL.

Bundled third-party material retains its upstream copyright, attribution, and license terms. It is **not** relicensed as first-party Gnosix source.

`vendor/blobin-ui-core` is first-party: the headless component tree of blob.in
(the operator's own project), vendored so a clean checkout builds without the
private blob.in repository. It is MIT like the rest of Gnosix (operator
decision, 2026-10-21).

## `vendor/gpui-pre-linux-0.3.5`

- Package: `gpui-pre-linux` 0.3.5
- Origin: Zed `gpui_linux` / `gpui-pre` snapshot
- Upstream repository: `https://github.com/zed-industries/zed`
- Upstream revision: `d89e9c2124b2786a390c7a451c7488601b4da2e1`
- License: **Apache-2.0**
- Bundled license text: [`vendor/gpui-pre-linux-0.3.5/LICENSE-APACHE`](vendor/gpui-pre-linux-0.3.5/LICENSE-APACHE)

## `vendor/gpui-pre-0.3.5`

- Package: `gpui-pre` 0.3.5
- Origin: Zed `gpui` snapshot (published as `gpui-pre`)
- Upstream repository: `https://github.com/zed-industries/zed`
- License: **Apache-2.0**
- Bundled license text: [`vendor/gpui-pre-0.3.5/LICENSE-APACHE`](vendor/gpui-pre-0.3.5/LICENSE-APACHE)

## `crates/gnos-ux-gpui/assets/fonts/gnosix-reference-sans-01f`

- Font: `GnosixReferenceSans01F-Regular.ttf`, a static instance derived from
  Google Sans Flex (fontTools `varLib.instancer`; the axes and the source's
  sha256 are in `derivation.json` beside it), renamed as the license requires.
- License: **SIL Open Font License 1.1**
- Bundled license text: [`LICENSE-OFL-1.1.txt`](crates/gnos-ux-gpui/assets/fonts/gnosix-reference-sans-01f/LICENSE-OFL-1.1.txt)

Dependencies resolved through Cargo keep their own upstream licenses and are not copied into this repository merely because they appear in `Cargo.lock`.

When adding vendored or copied third-party source, preserve its required notices and add it to this index before publication.