# Verified VPS publication

`animex-extensions-sync.py` uses root's existing `gh` authentication, selects the
newest successful `publish.yml` run from `main` (push/workflow_dispatch only), and
downloads its `vps-extensions` artifact. No token is stored in these files.

The public key in the service is pinned from `repository-key.json`. Rotation is
an explicit administrator action: change the unit's `--public-key` argument only
after independently authenticating the replacement key.

Verification covers repository identity, pinned descriptor key, SDK canonical
Ed25519 index signature, exact HTTPS module URL base, unique IDs and filenames,
package sizes/hashes, ZIP members, packaged manifests and module signatures.
Only regular `repo.json`, `index.json` and listed `.anmx` files are published.
Limits: 64 MiB/package and expanded WASM, 8 MiB/index and expanded manifest,
1 GiB total artifact, 5000 entries. No downloaded code is executed.

## Installation (administrator; not executed by preparation)

Requires `/usr/bin/gh`, `/usr/bin/python3`, `/usr/bin/openssl` with Ed25519 support,
root's working `gh auth`, and an existing `/var/www/senkai.fun` directory.

```sh
install -d -m 0755 /usr/local/libexec /var/www/senkai.fun/extensions-releases
install -m 0755 deploy/animex-extensions-sync.py /usr/local/libexec/
install -m 0644 deploy/animex-extensions-sync.service deploy/animex-extensions-sync.timer /etc/systemd/system/
# If gh auth uses a non-default GH_CONFIG_DIR, add a local service drop-in:
# /etc/systemd/system/animex-extensions-sync.service.d/github-auth.conf
# [Service]
# Environment="GH_CONFIG_DIR=/path/to/existing/gh/config"
# This path is local only; never copy hosts.yml into the repository.
systemctl daemon-reload
systemctl start animex-extensions-sync.service
systemctl enable --now animex-extensions-sync.timer
journalctl -u animex-extensions-sync.service -n 30 --no-pager
```

The live directory must be absent or a symlink into `extensions-releases`; an
existing real directory is deliberately not replaced. nginx must serve
`/var/www/senkai.fun/public/extensions`. The service uses root only for existing
auth and publishing rights; systemd makes host files read-only except the site,
`/run` and private temporary storage.

Each verified run becomes `extensions-releases/<run-id>`. A symlink rename
publishes it atomically. Existing same-run publication is skipped. No old
release is deleted. Forward deployments reject decreasing run IDs/revisions.

## Rollback

First stop the timer and wait for any active sync to finish. Choose an existing,
previously verified run directory (replace `PREVIOUS_RUN_ID` below), then:

```sh
systemctl stop animex-extensions-sync.timer
systemctl stop animex-extensions-sync.service
ln -s /var/www/senkai.fun/extensions-releases/PREVIOUS_RUN_ID /var/www/senkai.fun/public/.extensions-rollback
mv -Tf /var/www/senkai.fun/public/.extensions-rollback /var/www/senkai.fun/public/extensions
```

Leave the timer stopped while investigating; restarting it restores the newest
eligible successful run. Retention is manual; never remove the live target.

## Verification and operational boundaries

```sh
/usr/bin/python3 -m py_compile deploy/animex-extensions-sync.py deploy/test_sync.py
/usr/bin/python3 deploy/test_sync.py
systemd-analyze verify deploy/animex-extensions-sync.service deploy/animex-extensions-sync.timer
```

Tests require Python `cryptography` for fixture generation only (production uses
stdlib + system OpenSSL). Tests use real synthetic Ed25519 signatures and reject altered package contents,
index signatures, signed foreign/traversal/query URLs, extra symlinks and invalid
module signatures. No GitHub network calls or production deployment occur in tests.

`gh run download` performs artifact extraction using the installed GitHub CLI;
keep it current. The script restricts downloader file size/time and validates
all extracted files before publication, but the aggregate 1 GiB limit is checked
after extraction, not a filesystem quota. Use a bounded temporary filesystem
if hostile workflow artifacts are part of the threat model. Downloaded packages
are never extracted onto the site. Failure leaves the previous live symlink.
API queries inspect the newest 100 successful workflow runs/artifacts; if none
qualifies (or artifacts expired), deployment fails closed. GitHub/gh availability
and live nginx delivery require separate integration checks.

## HTTPS prerequisite

At preparation time, `senkai.fun` DNS returns SERVFAIL; HTTP is configured but
HTTPS certificate issuance is blocked. Repair authoritative DNS, set `A @` to
`188.137.232.177`, and remove any incorrect AAAA. Then obtain the certificate:

```sh
certbot certonly --webroot -w /var/www/senkai.fun/acme -d senkai.fun --non-interactive --agree-tos --register-unsafely-without-email
```

Back up the active nginx site configuration, replace it with
`deploy/senkai.fun.https.conf`, run `nginx -t`, then reload nginx. Do not alter
`openclaw.senkai.fun`. Keep the published signed descriptor and index URLs HTTPS;
clients cannot use the catalog until DNS and TLS work.
