# RPM spec for Zyzoom Download Manager (ZDM)
# Developed by Ramy Badraan
#
# Built by packaging/build-linux.sh, which stages the payload as a tarball whose
# root maps to /usr. Kept in the repo so Fedora/Copr and RHEL builds are
# reproducible without the packaging script.

%global debug_package %{nil}
%global __strip /bin/true

Name:           zyzoom-download-manager
Version:        %{_version}
Release:        1%{?dist}
Summary:        Multi-segment download accelerator with browser capture
License:        Proprietary
URL:            https://zyzoom.net
Source0:        %{name}-%{version}.tar.gz
BuildArch:      x86_64

Requires:       webkit2gtk4.1
Requires:       gtk3
Requires:       libayatana-appindicator-gtk3
Recommends:     yt-dlp
Recommends:     ffmpeg-free

%description
Zyzoom Download Manager (ZDM) downloads files over multiple parallel
connections and resumes interrupted transfers from the exact byte where they
stopped, instead of starting over or producing a corrupt file.

It also captures downloads from Chrome, Chromium, Brave, Edge, Vivaldi, Firefox
and Zen through a native messaging host, and downloads videos with yt-dlp.

A systemd user service plus XDG autostart entries keep it running in the tray
after login on every desktop, including Hyprland and sway.

%prep
%setup -q -c

%build
# Binaries are prebuilt by packaging/build-linux.sh.

%install
mkdir -p %{buildroot}/usr
cp -a usr/. %{buildroot}/usr/

%post
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t /usr/share/icons/hicolor >/dev/null 2>&1 || :
fi
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q /usr/share/applications >/dev/null 2>&1 || :
fi

%preun
if [ "$1" = 0 ]; then
  systemctl --user stop zdm.service >/dev/null 2>&1 || :
  pkill -x zdm >/dev/null 2>&1 || :
  pkill -x zdm-host >/dev/null 2>&1 || :
fi

%postun
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t /usr/share/icons/hicolor >/dev/null 2>&1 || :
fi

%files
/usr/bin/zdm
/usr/bin/zdm-host
/usr/lib/zdm
/usr/share/zdm
/usr/share/applications/zdm.desktop
/usr/share/icons/hicolor/*/apps/zdm.png
/usr/share/icons/hicolor/scalable/apps/zdm.svg
/usr/share/doc/zyzoom-download-manager/copyright

%changelog
* Sat Oct 24 2026 Ramy Badraan <ramy@zyzoom.net> - %{version}-1
- Rewritten download engine: strict 206 validation, real pause/resume.
- Fixed single-instance, tray autostart and browser integration.
- Added Arabic, English and French localisation.
