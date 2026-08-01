# `vale-ls`

The Vale Language Server (`vale-ls`) is an implementation of the 
[Language Server Protocol][1] (LSP) for [Vale][2].

It provides high-level interface for managing Vale and its assets
(binary, `StylesPath`, etc.) with the goal of making it easy to add IDE-like 
features to any text editor that supports the Language Server Protocol.

See the [documentation][4] for more information.

## Features

<table>
    <tr>
        <th>HoverProvider</th>
        <th>CompletionProvider</th>
    </tr>
    <tr>
        <td width="50%">
            <a href="https://user-images.githubusercontent.com/8785025/234143355-c442cbbd-ffc8-445f-a9b8-c3756ac1a5c2.png">
                <img src="https://user-images.githubusercontent.com/8785025/234143355-c442cbbd-ffc8-445f-a9b8-c3756ac1a5c2.png" width="100%">
            </a>
        </td>
        <td width="50%">
            <a href="https://user-images.githubusercontent.com/8785025/234143446-5dcb1f37-7af0-4834-84ca-37bb1db68f1e.png">
                <img src="https://user-images.githubusercontent.com/8785025/234143446-5dcb1f37-7af0-4834-84ca-37bb1db68f1e.png" width="100%">
            </a>
        </td>
    </tr>
    <tr>
        <td width="50%">
          See in-editor documentation for any symbol.
        </td>
        <td width="50%">Autocomplete all <code>StylesPath</code> assets: Styles, Packages, Vocabularies, etc.</td>
    </tr>
    <tr>
        <th>DocumentLinkProvider</th>
        <th>CodeActionProvider</th>
    </tr>
    <tr>
        <td width="50%">
            <a href="https://user-images.githubusercontent.com/8785025/234143624-a6125229-fc74-4051-a40a-92ede8861ab9.png">
                <img src="https://user-images.githubusercontent.com/8785025/234143624-a6125229-fc74-4051-a40a-92ede8861ab9.png" width="100%">
            </a>
        </td>
        <td width="50%">
            <a href="https://user-images.githubusercontent.com/8785025/234143654-d23a42a4-15d3-48cd-95cf-901d9b424b6b.png">
                <img src="https://user-images.githubusercontent.com/8785025/234143654-d23a42a4-15d3-48cd-95cf-901d9b424b6b.png" width="100%">
            </a>
        </td>
    </tr>
    <tr>
        <td width="50%">
          Quickly navigate to external URLs.
        </td>
        <td width="50%">
            Fix alerts with a single click.
        </td>
    </tr>
</table>

## Releasing

Releases are cut with [`cargo release`][5], which bumps `Cargo.toml` and
`Cargo.lock`, commits, tags, and pushes:

```console
$ cargo release patch --execute
```

Pushing the tag triggers [the release workflow](.github/workflows/release.yml),
which builds the binaries. Don't tag by hand: `vale-ls --version` reports the
version in `Cargo.toml`, so a tag that doesn't match it ships a binary that
reports the wrong version. The workflow now rejects that.

[1]: https://microsoft.github.io/language-server-protocol/
[2]: https://github.com/errata-ai/vale
[3]: https://github.com/errata-ai/vale-ls/releases
[4]: https://vale.sh/docs/guides/lsp
[5]: https://github.com/crate-ci/cargo-release
