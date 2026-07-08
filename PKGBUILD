# Maintainer: Mintori09
# Contributor: Mintori09

pkgname=anyrun-fork
pkgver=25.12.0
pkgrel=1
pkgdesc="A Wayland-native runner, similar to KRunner, with extreme customizability - includes extra plugins"
arch=('x86_64')
url="https://github.com/Mintori09/anyrun-fork"
license=('GPL3')
depends=(
    'cairo'
    'gdk-pixbuf2'
    'gtk4'
    'gtk4-layer-shell'
    'pango'
)
makedepends=(
    'cargo'
)
provides=('anyrun')
conflicts=('anyrun')
source=("$pkgname-$pkgver.tar.gz::https://github.com/Mintori09/anyrun-fork/archive/v$pkgver.tar.gz")
sha256sums=('SKIP')

_dirname="anyrun-fork-v$pkgver"

build() {
    cd "$srcdir/$_dirname"
    export CARGO_BUILD_JOBS=$(nproc)
    cargo build --release --frozen --workspace
}

package() {
    cd "$srcdir/$_dirname"

    # Binaries
    install -Dm755 target/release/anyrun -t "$pkgdir/usr/bin"

    # Plugins
    install -Dm755 target/release/*.so -t "$pkgdir/usr/lib/anyrun"
}
