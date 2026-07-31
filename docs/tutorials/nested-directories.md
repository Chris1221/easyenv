# Tutorial: nested directories and precedence

Real projects often have a repo-wide `.env` plus per-service overrides. easyenv walks from your current directory up to the filesystem root, loads every `.env` it finds, and merges them so that **closer directories win on conflicting keys**, while non-conflicting keys from parent directories still come through.

## Set up the fixture

```console
$ mkdir -p ~/easyenv-demo/api
$ cd ~/easyenv-demo

$ cat > .env <<'EOF'
SHARED_TOKEN=repo-wide-token
LOG_LEVEL=info
EOF

$ cat > api/.env <<'EOF'
LOG_LEVEL=debug
DATABASE_URL=postgres://localhost/api_dev
EOF
```

## Observe the merge

At the repo root, only the root `.env` is active:

```console
$ cd ~/easyenv-demo
$ echo "SHARED_TOKEN=$SHARED_TOKEN LOG_LEVEL=$LOG_LEVEL DATABASE_URL=[$DATABASE_URL]"
SHARED_TOKEN=repo-wide-token LOG_LEVEL=info DATABASE_URL=[]
```

Step into `api/`, and its `.env` is layered on top:

```console
$ cd api
$ echo "SHARED_TOKEN=$SHARED_TOKEN LOG_LEVEL=$LOG_LEVEL DATABASE_URL=$DATABASE_URL"
SHARED_TOKEN=repo-wide-token LOG_LEVEL=debug DATABASE_URL=postgres://localhost/api_dev
```

Notice:

- `SHARED_TOKEN` came from the parent `.env` — `api/.env` doesn't mention it, so it passes through unchanged.
- `LOG_LEVEL` is `debug`, not `info` — the closer `.env` in `api/` overrode the parent's value.
- `DATABASE_URL` only exists here, since only `api/.env` defines it.

You can confirm the origin of each variable with `easyenv status`:

```console
$ easyenv status
Active .env files for /home/you/easyenv-demo/api (root-first):
  /home/you/easyenv-demo/.env
  /home/you/easyenv-demo/api/.env

Resolved variables:
  DATABASE_URL=postgres://localhost/api_dev  (from /home/you/easyenv-demo/api/.env)
  LOG_LEVEL=debug  (from /home/you/easyenv-demo/api/.env)
  SHARED_TOKEN=repo-wide-token  (from /home/you/easyenv-demo/.env)
```

## Step back out

```console
$ cd ..
$ echo "LOG_LEVEL=$LOG_LEVEL DATABASE_URL=[$DATABASE_URL]"
LOG_LEVEL=info DATABASE_URL=[]
```

`LOG_LEVEL` is restored to the parent's `info` — not left at `debug`, and not unset entirely — and `DATABASE_URL` disappears, since nothing above `api/` defines it. This works no matter how many levels deep you go, or how many directories in the chain override the same key.

## Clean up

```console
$ cd ~
$ rm -rf ~/easyenv-demo
```
