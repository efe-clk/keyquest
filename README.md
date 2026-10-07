# Homerow (çalışma adı)

Linux masaüstü için 10 parmak klavye alıştırması. Her karakter için hangi tuşa,
hangi elin hangi parmağıyla basılacağını gösterir; Türkçe Q ve Türkçe F
düzenlerini birinci sınıf destekler. Mimari ve yol haritası:
[`docs/blueprint.md`](docs/blueprint.md).

## Durum

| Aşama (blueprint 16) | Durum |
|---|---|
| 1. Çekirdek: `Session`, `Metrics`, `Layout`, `stroke_for`, testler | ✔ |
| 2. İlk arayüz (GTK 4 + libadwaita) | Bekliyor. Şimdilik terminal arayüzü (`homerow-cli`) var |
| 3. Parmak rehberi (sanal klavye, el çizimi) | Bekliyor. Terminalde metin olarak gösteriliyor |
| 4. Kalıcılık: SQLite, migration'lar, ders kilidi, ayarlar | Veri katmanı ✔ (ayar ekranı GTK ile gelecek) |
| 5. İstatistik ve zayıf tuş modu | Çekirdek ve terminal ✔ |
| 6. Türkçe F / US düzenleri, kullanıcı TOML dizini, CI | Düzenler, kullanıcı dizini ve CI ✔; Flatpak bekliyor |

## Yapı

```
crates/
  homerow-core/   UI ve depolamadan bağımsız çekirdek (oturum, ölçüm, düzen, ders üretimi)
  homerow-data/   Gömülü + kullanıcı TOML dosyaları, ayarlar, SQLite ilerleme kaydı
    data/layouts/   tr-q, tr-f, us
    data/lessons/   Türkçe Q dersleri
    migrations/     SQL şema migration'ları
  homerow-cli/    Terminal arayüzü
docs/blueprint.md
```

Bağımlılık yönü içeri doğrudur: `homerow-cli` (ileride GTK arayüzü) →
`homerow-core` ← `homerow-data`.

## Terminalde kullanım

```sh
cargo run -p homerow-cli -- practice            # sıradaki açık ders
cargo run -p homerow-cli -- practice --weak     # zayıf tuş alıştırması
cargo run -p homerow-cli -- practice --text "Şu an yazıyorum."
cargo run -p homerow-cli -- lessons             # dersler ve kilit durumu
cargo run -p homerow-cli -- stroke "Ağaç@"      # tuş / parmak / değiştirici
cargo run -p homerow-cli -- stats               # gelişim, zayıf tuşlar ve parmaklar
cargo run -p homerow-cli -- --layout tr-f check # düzen ve ders dosyalarını doğrula
```

Alıştırma sırasında: `Esc` duraklatır, `Ctrl+C` iptal eder. Sistem klavye
düzeninizin seçilen düzenle aynı olması gerekir (karşılaştırma üretilen
karaktere göre yapılır, KR-5).

## Dosyalar

| Veri | Konum |
|---|---|
| Ayarlar | `~/.config/homerow/config.toml` |
| İlerleme | `~/.local/share/homerow/progress.db` (yedek: `progress.db.bak`) |
| Kullanıcı düzenleri | `~/.local/share/homerow/layouts/*.toml` |
| Kullanıcı dersleri | `~/.local/share/homerow/lessons/<düzen>/*.toml` |

Kullanıcı dosyası, aynı `id`'ye sahip gömülü dosyanın yerini alır. Hatalı
dosyalar uyarıyla atlanır. Format örnekleri `crates/homerow-data/data/` altında.

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
```

Yeni gömülü düzen veya ders dosyası eklerken `crates/homerow-data/src/embedded.rs`
listesini de güncelleyin; bir test bunun unutulmasını yakalar.
