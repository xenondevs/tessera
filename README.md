<img align="right" width="160" src=".github/assets/logo.avif" alt="Tessera logo">
<br>
<picture>
  <source media="(prefers-color-scheme: dark)" srcset=".github/assets/wordmark-dark.png">
  <img width="218" src=".github/assets/wordmark-light.png" alt="Tessera">
</picture>
<br><br>
Tessera is a Minecraft rendering library implementing the entire pipeline from scratch. It currently supports items, block states and raw models with planned support for
more. <sup><a href="#Supported-renders">(See below)</a></sup>
<br><br>
<p align="center">
  <img alt="Library version" src="https://img.shields.io/github/v/release/xenondevs/tessera?filter=!capture-*&label=library&color=green">
  <img alt="Mod version" src="https://img.shields.io/github/v/release/xenondevs/tessera?filter=capture-*&label=capture%20mods&color=green">
  <br>
  <img alt="Docs" src="https://img.shields.io/badge/docs-coming_soon-orange">
</p>
<br clear="all">

## Supported renders

* items
* block states
* tile entities
* models
* ~~itemstacks~~ planned
* ~~scenes~~ - planned
* ~~entities~~ - planned
* ~~structure files~~ - planned

## Version Compatibility

Each library version only targets a specific Minecraft version in order to keep the crate slim and to be able to always ship the latest embedded block entity dumps.

| Minecraft version    | Tessera version   |
|----------------------|-------------------|
| `26.3`               | `0.1.2`           |

## Gradle/Maven/Cargo

**Regarding JVM usage: Please note that Tessera's module will need native access in order to load the native parts, this is enabled by default for all unnamed modules in newer Paper versions**

### Gradle
```kt
maven {
    name = "xenondevsReleases"
    url = uri("https://repo.xenondevs.xyz/releases")
}
// ...
implementation("xyz.xenondevs.tessera:tessera:0.1.2")
```

### Maven
```xml
<repository>
  <id>xenondevs-releases</id>
  <name>xenondevs repository</name>
  <url>https://repo.xenondevs.xyz/releases</url>
</repository>
<!-- ... -->
<dependency>
  <groupId>xyz.xenondevs.tessera</groupId>
  <artifactId>tessera</artifactId>
  <version>0.1.2</version>
</dependency>
```

### Cargo

**Tessera is currently not published to crates.io, so you will need to point to the repo**

```toml
tessera = { git = "https://github.com/xenondevs/tessera", tag = "0.1.2" }
```

## Attestation

To prevent supply chain attacks, every Tessera jar ships with a list of allowed hashes for each native library variant it could use. Each of those natives and the jar itself are attested via GitHub which allows you to verify those files were actually built in GitHub actions. To verify a file from an official release, use the gh cli: `gh attestation verify --owner xenondevs <file>` or check out https://github.com/xenondevs/tessera/attestations.  
All [workflow actions](https://github.com/xenondevs/tessera/blob/master/.github/workflows/actions.lock) + [dependencies](https://github.com/xenondevs/tessera/blob/master/native/Cargo.lock) are also hash pinned including shipped [gradle](https://github.com/xenondevs/tessera/blob/master/gradle/wrapper/gradle-wrapper.properties#L3) [dependencies and plugins](https://github.com/xenondevs/tessera/blob/master/gradle/verification-metadata.xml).
