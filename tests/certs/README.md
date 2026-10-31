# Test certificates

`analyzer-test-cert.pem` / `analyzer-test-key.pem` is a self-signed
certificate for `localhost` and `127.0.0.1`, used only by the
analyzer listener tests to exercise the real TLS code path.  It is
not used anywhere in production code and carries no authority.
Regenerate with:

    openssl req -x509 -newkey rsa:2048 -nodes \
      -keyout analyzer-test-key.pem -out analyzer-test-cert.pem \
      -days 3650 -subj "/CN=localhost" \
      -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"
