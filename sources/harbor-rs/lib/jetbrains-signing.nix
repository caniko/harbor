# Build a private, encrypted JetBrains signing key and self-signed chain.
#
# This is an executable helper rather than a derivation containing generated
# secrets. Callers must provide a private output directory; the helper refuses
# to overwrite an existing path and prints only public paths and a fingerprint.
{pkgs}:
pkgs.writeShellApplication {
  name = "generate-jetbrains-signing-material";
  runtimeInputs = [pkgs.coreutils pkgs.openssl];
  text = ''
    set -euo pipefail

    usage() {
      echo "usage: generate-jetbrains-signing-material OUTPUT_DIR [SUBJECT] [DAYS]" >&2
    }

    if [ "$#" -lt 1 ] || [ "$#" -gt 3 ]; then
      usage
      exit 2
    fi

    output_dir=$1
    subject=''${2:-/CN=JetBrains Plugin Signing}
    days=''${3:-3650}

    case "$output_dir" in
      ""|/nix/store/*)
        echo "output directory must be a non-store path" >&2
        exit 2
        ;;
    esac

    if [ -e "$output_dir" ]; then
      echo "refusing to overwrite existing path: $output_dir" >&2
      exit 1
    fi

    umask 077
    mkdir -p "$output_dir"
    chmod 700 "$output_dir"
    password_file="$output_dir/private-key-password"
    private_key_file="$output_dir/private-key.pem"
    certificate_chain_file="$output_dir/certificate-chain.pem"

    openssl rand -base64 48 > "$password_file"
    openssl genpkey \
      -algorithm RSA \
      -aes-256-cbc \
      -pkeyopt rsa_keygen_bits:4096 \
      -pass file:"$password_file" \
      -out "$private_key_file" \
      >/dev/null 2>&1
    openssl req \
      -new \
      -x509 \
      -sha256 \
      -days "$days" \
      -subj "$subject" \
      -key "$private_key_file" \
      -passin file:"$password_file" \
      -out "$certificate_chain_file" \
      >/dev/null 2>&1

    chmod 600 "$password_file" "$private_key_file"
    chmod 644 "$certificate_chain_file"

    fingerprint=$(openssl x509 -in "$certificate_chain_file" -noout -fingerprint -sha256)
    echo "certificate_chain=$certificate_chain_file"
    echo "private_key=$private_key_file"
    echo "private_key_password=$password_file"
    echo "$fingerprint"
  '';
}
