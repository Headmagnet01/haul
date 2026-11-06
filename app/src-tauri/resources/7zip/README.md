# Bundled 7-Zip

Haul ships these files to extract archives (zip, 7z, RAR, split sets, password-protected ones). They are unmodified copies from the official 7-Zip release. See `License.txt` (GNU LGPL, with the unRAR licence restriction for the RAR code in `7z.dll`).

- Version: 7-Zip 26.05 (x64), released 2026-10-21, by Igor Pavlov.
- Source: <https://www.7-zip.org/download.html>, which links to <https://github.com/ip7z/7zip/releases/download/26.05/7z2605-x64.exe>.
- How they were taken out: `7zr.exe x 7z2605-x64.exe` (`7zr.exe` from the same release page).
- The releases are not Authenticode-signed. Check the hashes below instead.

| File | SHA-256 |
|---|---|
| `7z2605-x64.exe` (installer) | `fd99e86f57d8f429ef5ed045bf98315af962e2309b0f1ba87a147a44c71647d0` |
| `7zr.exe` | `9bf582b894dfa665f852d7e9112b02290a1c303c2d95a21fcdc41ffcb03143b7` |
| `7z.exe` | `94205cfc054dd3bbeb978905bc619a65b3f04e9e73bb2717841110b6f46d962e` |
| `7z.dll` | `78bf6b668d75801e3aa180b4ccb16e6d8395fba80dec8a065055bb6f8dfabb82` |
| `License.txt` | `519ac0a4bded9c18ea02e0afb71f663d8c47373bd9facd3ac96a79f51d77765d` |
