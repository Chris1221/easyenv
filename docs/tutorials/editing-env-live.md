# Tutorial: editing a `.env` while you're in its directory

easyenv doesn't just react to `cd` — it also notices when a `.env` file changes while you're already sitting in that directory, and picks up the change on your very next command.

## Set up

```console
$ mkdir ~/easyenv-demo
$ cd ~/easyenv-demo
$ echo 'FEATURE_FLAG=off' > .env
$ echo $FEATURE_FLAG
off
```

## Edit the file without leaving

Open `.env` in another terminal, or append to it directly:

```console
$ echo 'FEATURE_FLAG=on' > .env
```

You don't need to `cd .` or open a new shell — just run any command (even a blank `Enter`) and the new value is there:

```console
$ echo $FEATURE_FLAG
on
```

## Why this works

Every time your shell is about to draw a prompt, it asks easyenv "has anything relevant changed?" easyenv checks the current directory's `.env` files' modification times and sizes — not their content — as a cheap signature. If that signature differs from what it saw last time, it re-reads the files and re-applies the diff; if not, it exits immediately without touching anything. Editing the file changes its modification time, so the very next prompt notices.

See [How it works](../reference/how-it-works.md) for the full design.

## Clean up

```console
$ cd ~
$ rm -rf ~/easyenv-demo
```
