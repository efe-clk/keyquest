# KeyQuest

Linux masaüstü için 10 parmak klavye alıştırması. Her karakter için hangi tuşa,
hangi elin hangi parmağıyla basılacağını gösterir; Türkçe Q ve Türkçe F
düzenlerini birinci sınıf destekler. Mimari ve yol haritası:
[`docs/blueprint.md`](docs/blueprint.md).

## Durum

| Aşama (blueprint 16) | Durum |
|---|---|
| 1. Çekirdek: `Session`, `Metrics`, `Layout`, `stroke_for`, testler | ✔ |
| 2. İlk arayüz: ana pencere, alıştırma, sonuç ekranı, Türkçe Q dersleri | ✔ |
| 3. Parmak rehberi: sanal klavye, el çizimi, Shift/AltGr vurgusu | ✔ |
| 4. Kalıcılık: SQLite, migration'lar, ders kilidi, ayarlar | ✔ |
| 5. İstatistik ekranı ve zayıf tuş modu | ✔ |
| 6. Türkçe F / US düzenleri, kullanıcı TOML dizini, CI, AppImage | ✔ |

## Yapı

```
crates/
  keyquest-core/   UI ve depolamadan bağımsız çekirdek (oturum, ölçüm, düzen, ders üretimi)
  keyquest-data/   Gömülü + kullanıcı TOML dosyaları, ayarlar, SQLite ilerleme kaydı
    data/layouts/    tr-q, tr-f, us
    data/lessons/    Ders dosyaları (düzen başına bir dizin)
    migrations/      SQL şema migration'ları
  keyquest-gtk/    Masaüstü uygulaması (GTK 4 + libadwaita), `keyquest` komutu
  keyquest-cli/    Terminal arayüzü ve veri dosyası doğrulama, `keyquest-cli` komutu
data/              .desktop dosyası, AppStream metainfo, simge
build-aux/         AppImage derleme betiği
docs/blueprint.md
```

Bağımlılık yönü içeri doğrudur: `keyquest-gtk` / `keyquest-cli` →
`keyquest-core` ← `keyquest-data`.

## Masaüstü uygulaması

Gerekenler: GTK ≥ 4.14 ve libadwaita ≥ 1.5 geliştirme paketleri
(Debian/Ubuntu: `libgtk-4-dev libadwaita-1-dev`, Fedora: `gtk4-devel libadwaita-devel`).

```sh
cargo run -p keyquest-gtk
```

- Ders listesi: dersler sırayla açılır; "Devam et" sıradaki açık derse götürür.
- Alıştırma: sıradaki tuş sanal klavyede, sorumlu parmak el çiziminde vurgulanır.
  Büyük harf ve AltGr karakterlerinde değiştirici tuş ve onu tutan parmak da
  vurgulanır (kesik çizgi). Tuşlar parmak rengiyle boyanır ve parmak etiketi taşır.
- `Esc` duraklatır; pencere odağı kaybedilince oturum kendiliğinden duraklar.
- Sistem klavye düzeni seçilen düzenden farklı görünüyorsa üstte uyarı çıkar.
- Menü: İstatistikler (net hız grafiği, en zayıf tuşlar ve parmaklar), Ayarlar
  (düzen, boşluk başparmağı, hata davranışı, tema, metin boyutu, ilerlemeyi
  JSON olarak dışa/içe aktarma ve sıfırlama).

## AppImage

```sh
./build-aux/appimage.sh          # target/KeyQuest-<sürüm>-x86_64.AppImage
chmod +x KeyQuest-*.AppImage && ./KeyQuest-*.AppImage
```

Betik linuxdeploy ve GTK eklentisini `target/appimage-tools/` altına indirir,
GTK 4 ve libadwaita'yı pakete gömer. AppImage, derlendiği sistemdeki glibc ile
aynı veya daha yeni glibc'ye sahip dağıtımlarda çalışır (Ubuntu 24.04'te
derlenirse glibc ≥ 2.39). Wayland ve X11 oturumlarında doğrudan çalışır; GTK
veya Adwaita simge teması kurulu olmayan sistemlerde de denendi. `v*` etiketi gönderildiğinde CI AppImage'ı derleyip
GitHub Release'e ekler. Aynı iş akışı GitHub'da elle de başlatılabilir
(Actions → AppImage → Run workflow); "tag" alanına `v0.1.0` gibi bir sürüm
yazılırsa etiket ve Release o commit'ten oluşturulur.

## Terminalde kullanım

```sh
cargo run -p keyquest-cli -- practice            # sıradaki açık ders
cargo run -p keyquest-cli -- practice --weak     # zayıf tuş alıştırması
cargo run -p keyquest-cli -- practice --text "Şu an yazıyorum."
cargo run -p keyquest-cli -- lessons             # dersler ve kilit durumu
cargo run -p keyquest-cli -- stroke "Ağaç@"      # tuş / parmak / değiştirici
cargo run -p keyquest-cli -- stats               # gelişim, zayıf tuşlar ve parmaklar
cargo run -p keyquest-cli -- --layout tr-f check # düzen ve ders dosyalarını doğrula
cargo run -p keyquest-cli -- export yedek.json  # ilerlemeyi JSON'a aktar
cargo run -p keyquest-cli -- import yedek.json --yes  # yedekten geri yükle
```

Alıştırma sırasında: `Esc` duraklatır, `Ctrl+C` iptal eder. Sistem klavye
düzeninizin seçilen düzenle aynı olması gerekir (karşılaştırma üretilen
karaktere göre yapılır, KR-5).

## Dosyalar

| Veri | Konum |
|---|---|
| Ayarlar | `~/.config/keyquest/config.toml` |
| İlerleme | `~/.local/share/keyquest/progress.db` (yedek: `progress.db.bak`) |
| Kullanıcı düzenleri | `~/.local/share/keyquest/layouts/*.toml` |
| Kullanıcı dersleri | `~/.local/share/keyquest/lessons/<düzen>/*.toml` |

Kullanıcı dosyası, aynı `id`'ye sahip gömülü dosyanın yerini alır. Hatalı
dosyalar uyarıyla atlanır. Format örnekleri `crates/keyquest-data/data/` altında.

`config.toml` örneği:

```toml
layout = "tr-q"          # tr-q, tr-f, us
space_thumb = "right"    # boşluk tuşu başparmağı: left / right
error_mode = "stop-on-error"  # veya "continue"
theme = "system"         # system / light / dark
```

## Geliştirme

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo audit   # bilinen güvenlik açığı olan bağımlılık var mı (CI'da da çalışır)
```

Log seviyesi `RUST_LOG` ile seçilir, örneğin `RUST_LOG=info cargo run -p keyquest-gtk`.

Yeni gömülü düzen veya ders dosyası eklerken `crates/keyquest-data/src/embedded.rs`
listesini de güncelleyin; bir test bunun unutulmasını yakalar.
