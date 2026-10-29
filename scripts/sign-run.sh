#!/bin/sh
# Cargo runner for macOS: signs glkvm-gui with a stable self-signed identity, then runs it.
# The keychain trusts an app by its signature; an unsigned binary gets a new hash on every
# build and macOS asks for the keychain password again. Creates the identity on first use.
set -e
NAME="glkvm-dev"
bin="$1"
if [ "$(basename "$bin")" = glkvm-gui ] && [ "$(uname)" = Darwin ]; then
    if ! security find-certificate -c "$NAME" >/dev/null 2>&1; then
        d=$(mktemp -d)
        cat >"$d/o.cnf" <<CNF
[req]
distinguished_name=dn
x509_extensions=ext
prompt=no
[dn]
CN=$NAME
[ext]
keyUsage=critical,digitalSignature
extendedKeyUsage=critical,codeSigning
CNF
        /usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -config "$d/o.cnf" \
            -keyout "$d/k.pem" -out "$d/c.pem" 2>/dev/null
        /usr/bin/openssl pkcs12 -export -inkey "$d/k.pem" -in "$d/c.pem" -out "$d/i.p12" -passout pass:glkvm
        security import "$d/i.p12" -k "$HOME/Library/Keychains/login.keychain-db" \
            -P glkvm -T /usr/bin/codesign >/dev/null
        rm -rf "$d"
    fi
    codesign -f -s "$NAME" "$bin" >&2 || echo "sign-run: codesign failed, running unsigned" >&2
fi
exec "$@"
