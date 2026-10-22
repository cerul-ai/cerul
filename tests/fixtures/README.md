# Test fixtures

`ocr-text.png` is an original synthetic image created for this repository:
black system-font text `CERUL VIDEO 123` on a white background. It is distributed
under the repository Apache-2.0 license and contains no customer data.

`emudhra-dv-tls-g3b.der` and `emudhra-dv-tls-g3b-aaa.der` are the two public
issuances of eMudhra's `EM DV TLS CA - G3B` intermediate, signed by
`emSign Root TLS CA - G3` (crt.sh ID 18286207191) and cross-signed by
`AAA Certificate Services` (crt.sh ID 18519019583). Its private key was
disclosed in October 2026. They are published CA certificates, downloaded
unchanged from crt.sh, and are used only to test that such chains are refused.
