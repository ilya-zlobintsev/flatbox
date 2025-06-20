Flatbox is a utility that lets you run [Flatpak](https://flatpak.org/) applications with less restrictions compared to their usual sandbox.

The intended use case is to run a privileged part of an application, but running full applications is mostly functional as well.

## Installation

A static binary can be downloaded from the [releases page](https://github.com/ilya-zlobintsev/flatbox/releases).

## Usage

First, you need to have an application or a runtime installed through Flatpak normally. Flatbox will automatically search Flatpak system and user install directories.

Run an application with flatbox:
```
flatbox run --app io.github.arunsivaramanneo.GPUViewer gpu-viewer
```
Run a shell using a runtime:
```
flatbox run --runtime org.gnome.Platform/x86_64/48 bash
```

# Use cases and differences compared to running with Flatpak

When a Flatpak application is started, it creates a sandbox using `bwrap` which limits its access to various system interactions. The permissions of this sandbox are configurable, but some things (such as write access to `/sys`) are always disallowed.

Flatbox also uses `bwrap` to create a sandbox environment (with the usual flatpak runtime and extensions being used to run the application), but it exposes as much access to the host system as possible without interfering with the flatpak runtime.

In particular, the sandbox has the following
- Unrestricted access to `/home`, `/sys`, `/dev`, and other root paths that aren't otherwise part of the flatpak runtime
- Full host filesystem root at `/run/host/root`
- Information about users on the system

Additionally, flatbox does not interact with user sessions or Flatpak's helper services, making it suitable to be used in user or system services.
