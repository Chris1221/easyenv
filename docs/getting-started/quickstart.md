# Quickstart

This walks through the entire feature in about five minutes. Make sure you've completed [installation](installation.md) and restarted your shell first.

## Create a project with a `.env`

```console
$ mkdir -p ~/easyenv-demo
$ cd ~/easyenv-demo
$ cat > .env <<'EOF'
GREETING=hello from easyenv
API_KEY=demo-key-123
EOF
```

## Load automatically on `cd`

Leave the directory and come back — you don't need to run anything yourself:

```console
$ cd ~
$ echo "GREETING is: [$GREETING]"
GREETING is: []

$ cd ~/easyenv-demo
$ echo "GREETING is: [$GREETING]"
GREETING is: [hello from easyenv]
$ echo $API_KEY
demo-key-123
```

## Unload automatically on `cd` out

```console
$ cd ~
$ echo "GREETING is: [$GREETING]"
GREETING is: []
```

The variables are gone as soon as you leave — nothing lingers for the next project you `cd` into.

## Check what's active at any time

`easyenv status` shows which `.env` files are contributing to the current directory, without changing anything:

```console
$ cd ~/easyenv-demo
$ easyenv status
Active .env files for /home/you/easyenv-demo (root-first):
  /home/you/easyenv-demo/.env

Resolved variables:
  API_KEY=demo-key-123  (from /home/you/easyenv-demo/.env)
  GREETING=hello from easyenv  (from /home/you/easyenv-demo/.env)
```

## Clean up

```console
$ rm -rf ~/easyenv-demo
```

That's the core loop. Next: see how easyenv handles [nested directories](../tutorials/nested-directories.md) with their own `.env` files.
