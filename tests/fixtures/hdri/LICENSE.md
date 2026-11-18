# HDRI fixtures

| File | Source | License | Authors | Changes |
|---|---|---|---|---|
| `kloofendal_48d_partly_cloudy_puresky_512.hdr` | Poly Haven, "Kloofendal 48d Partly Cloudy (Pure Sky)": https://polyhaven.com/a/kloofendal_48d_partly_cloudy_puresky (1k `.hdr`, MD5 `c69498687e876bf68d2a8b8a62234eb9`) | CC0 1.0 (public domain dedication): https://polyhaven.com/license | Greg Zaal (original), Jarod Guest (sky edits) | Downsampled from 1024×512 to 512×256 by 2×2 box filtering in linear light; written as flat (uncompressed) RGBE. SHA-256 of the committed file: `f5d83736c2d2d8d999b92f67c289f57b4084f17249c1fa837d166c052a9cdab4`. |

Used by the image-based lighting tests and goldens in `crates/concordia-render` (M1 phase 1.6). Attribution is not legally required under CC0, but we credit the authors.
