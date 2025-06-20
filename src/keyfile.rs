use anyhow::Context;
use indexmap::IndexMap;

pub fn parse_keyfile(source: &str) -> anyhow::Result<IndexMap<&str, IndexMap<&str, &str>>> {
    let mut out = IndexMap::new();

    let mut lines = source
        .lines()
        .map(|line| line.trim_ascii())
        .enumerate()
        .filter(|(_, line)| !line.is_empty())
        .peekable();

    while let Some((i, line)) = lines.next() {
        let section_name = line
            .strip_prefix('[')
            .and_then(|name| name.strip_suffix(']'))
            .with_context(|| format!("Expected section name, found '{line}' at line {i}"))?;

        let mut section = IndexMap::new();

        while lines.peek().is_some_and(|(_, line)| !line.starts_with('[')) {
            let (i, line) = lines.next().unwrap();
            let (key, value) = line
                .split_once('=')
                .with_context(|| format!("Unexpected line format '{line}' at {i}"))?;

            section.insert(key.trim_ascii(), value.trim_ascii());
        }

        out.insert(section_name, section);
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use crate::keyfile::parse_keyfile;

    #[test]
    fn parse_gnome_metadata() {
        let data = "
[Runtime]
name = org.gnome.Platform
runtime = org.gnome.Platform/x86_64/48
sdk = org.gnome.Sdk/x86_64/48

[Environment]
GI_TYPELIB_PATH = /app/lib/girepository-1.0
GST_PLUGIN_SYSTEM_PATH = /app/lib/gstreamer-1.0:/usr/lib/extensions/gstreamer-1.0:/usr/lib/x86_64-linux-gnu/gstreamer-1.0
XDG_DATA_DIRS = /app/share:/usr/share:/usr/share/runtime/share:/run/host/user-share:/run/host/share
ALSA_CONFIG_DIR = /usr/share/alsa
ALSA_CONFIG_PATH = /usr/share/alsa/alsa-flatpak.conf
__EGL_EXTERNAL_PLATFORM_CONFIG_DIRS = /etc/egl/egl_external_platform.d:/usr/lib/x86_64-linux-gnu/GL/egl/egl_external_platform.d:/usr/share/egl/egl_external_platform.d
PYTHONUSERBASE = /var/data/python

[Extension org.gnome.Platform.Locale]
directory = share/runtime/locale

[Extension org.freedesktop.Platform.GL]
versions = 24.08;24.08extra;1.4
version = 1.4
directory = lib/x86_64-linux-gnu/GL
subdirectories = true
no-autodownload = true
autodelete = false
add-ld-path = lib
merge-dirs = vulkan/icd.d;glvnd/egl_vendor.d;egl/egl_external_platform.d;OpenCL/vendors;lib/dri;lib/d3d;lib/gbm;vulkan/explicit_layer.d;vulkan/implicit_layer.d;vdpau
download-if = active-gl-driver
enable-if = active-gl-driver
autoprune-unless = active-gl-driver

[Extension org.freedesktop.Platform.GL.Debug]
versions = 24.08;24.08extra;1.4
version = 1.4
directory = lib/debug/usr/lib/x86_64-linux-gnu/GL
subdirectories = true
no-autodownload = true
merge-dirs = vulkan/icd.d;glvnd/egl_vendor.d;egl/egl_external_platform.d;OpenCL/vendors;lib/dri;lib/d3d;lib/gbm;vulkan/explicit_layer.d;vulkan/implicit_layer.d;vdpau
enable-if = active-gl-driver
autoprune-unless = active-gl-driver

[Extension org.freedesktop.Platform.VulkanLayer]
version = 24.08
directory = lib/extensions/vulkan
subdirectories = true
no-autodownload = true
merge-dirs = share/vulkan/implicit_layer.d;share/vulkan/explicit_layer.d;

[Extension org.freedesktop.Platform.Timezones]
directory = share/zoneinfo
version = 24.08

[Extension org.freedesktop.Platform.GStreamer]
directory = lib/extensions/gstreamer-1.0
subdirectories = true
no-autodownload = true
version = 24.08

[Extension org.freedesktop.Platform.Icontheme]
directory = share/runtime/share/icons
subdirectories = true
no-autodownload = true
version = 1.0

[Extension org.gtk.Gtk3theme]
directory = share/runtime/share/themes
subdirectories = true
subdirectory-suffix = gtk-3.0
no-autodownload = true
version = 3.22
download-if = active-gtk-theme

[Extension org.freedesktop.Platform.VAAPI.Intel]
directory = lib/x86_64-linux-gnu/dri/intel-vaapi-driver
autodelete = false
no-autodownload = true
add-ld-path = lib
download-if = have-intel-gpu
autoprune-unless = have-intel-gpu
version = 24.08

[Extension org.freedesktop.Platform.openh264]
directory = lib/x86_64-linux-gnu/openh264
versions = 2.5.1beta;2.5.1
version = 2.5.1
add-ld-path = extra
autodelete = true
        ";

        let data = parse_keyfile(data).unwrap();
        assert_eq!(
            "org.gnome.Sdk/x86_64/48",
            *data.get("Runtime").unwrap().get("sdk").unwrap()
        );
        assert_eq!(
            "2.5.1beta;2.5.1",
            *data
                .get("Extension org.freedesktop.Platform.openh264")
                .unwrap()
                .get("versions")
                .unwrap()
        );
    }
}
