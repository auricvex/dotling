# dotling vault

Manage the encryption vault.

## Usage

```sh
dotling vault <ACTION>
```

## Actions

### `dotling vault init`

Initialize a new vault with a password.

```sh
dotling vault init
```

You'll be prompted for a password (entered twice for confirmation). This creates the vault at `~/.dotling/vault/` with an encrypted identity secret.

### `dotling vault show`

Display vault status and location.

```sh
dotling vault show
```

Shows whether a vault exists and its location. Note: creation date is not currently displayed.

### `dotling vault export`

Export the vault as a portable encrypted bundle.

```sh
dotling vault export <PATH>
```

Creates a single encrypted file containing the vault config and identity secret. Use this to migrate your vault to a new machine.

### `dotling vault import`

Import a vault bundle.

```sh
dotling vault import <PATH>
```

Imports the vault from an encrypted bundle. You'll be prompted for the bundle's password. This overwrites any existing vault.

### `dotling vault change-password`

Change the vault password.

```sh
dotling vault change-password
```

You'll be prompted for the current password and a new password. The vault identity is re-encrypted with the new password.

## Examples

```sh
# Set up encryption for the first time
dotling vault init

# Check vault status
dotling vault show

# Migrate to a new machine
dotling vault export ~/vault.bundle
# ... copy vault.bundle to new machine ...
dotling vault import ~/vault.bundle

# Change your password
dotling vault change-password
```

## See also

- [Encryption](../encryption.md) — full encryption guide
