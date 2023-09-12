# Setting up

Running a command like `list` will fail because no database exists yet

```console
$ vidl list
? failed
Error: unable to open database file: [..]
...
```

So initialize the database:

```console
$ vidl init
...
```

Running list now works, with no output:
```console
$ vidl list
```

Add a video

```console
$ vidl add thegreatsd
...
```

Running list now works, with no output:
```console
$ vidl list
1 - thegreatsd ([..] on service youtube)
Thumbnail: [..]

```

No videos yet:

```console
$ vidl list 1
```

Perform update:

```console
$ vidl update
```

Shows new videos:
```console
$ vidl list 1
ID: [..]
Title: [..]
URL: [..]
Published: [..]
Thumbnail: [..]
Description: [..]
----
ID: [..]
Title: [..]
...
```
