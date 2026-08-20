# Catalog benchmark fixture sources

The benchmark keeps the downloaded source image unchanged apart from source-provided thumbnail scaling. Ground-truth overlays live in `review.html`; they are not baked into model inputs. OCR fixture files 03 and 04 define deterministic runtime crops in `manifest.json` (difficulty levels 4 and 5); no generative image editing is used. Each OCR manifest entry also declares whether runtime bytes match a PNG screen selection or an unchanged dropped file.

| Fixture | Source | License / status | Local treatment |
| --- | --- | --- | --- |
| `coordinate/01-vlc.png` | [VLC Media Player screenshot](https://commons.wikimedia.org/wiki/File:VLC_Media_Player_Screenshot.png) | Free software screenshot (GPL) | Original file |
| `coordinate/02-office.png` | [LibreOffice 7.1 screenshot](https://commons.wikimedia.org/wiki/File:LibreOffice_7.1_screenshot.png) | Free software screenshot (MPL 2.0 / LGPL) | Original file |
| `coordinate/03-settings.png` | [GNOME Settings 3.32 screenshot](https://commons.wikimedia.org/wiki/File:GNOME_Settings_3.32_screenshot.png) | Free software screenshot (GPL) | Wikimedia 960 px thumbnail |
| `coordinate/04-vector-editor.png` | [Inkscape 1.2 screenshot](https://commons.wikimedia.org/wiki/File:Inkscape_1.2_screenshot.png) | Free software screenshot (GPL) | Original file |
| `coordinate/05-3d-editor.png` | [Blender 3.0 screenshot](https://commons.wikimedia.org/wiki/File:Blender_3.0.0_screenshot.png) | GPL screenshot; splash artwork credited on source page under CC BY 4.0 | Wikimedia 960 px thumbnail |
| `coordinate/06-duplicate-color.png` | [ScreenSpot test row 44](https://huggingface.co/datasets/bevaya/ScreenSpot) | Apache-2.0 dataset | Source-provided 960×540 image; ground-truth box converted from the normalized dataset box |
| `coordinate/07-mobile-controls.jpg` | [ScreenSpot test row 624](https://huggingface.co/datasets/bevaya/ScreenSpot) | Apache-2.0 dataset | Source-provided 2360×1640 image; ground-truth box converted from the normalized dataset box |
| `coordinate/08-filter-chip.png` | [ScreenSpot test row 861](https://huggingface.co/datasets/bevaya/ScreenSpot) | Apache-2.0 dataset | Source-provided 2560×1440 image; ground-truth box converted from the normalized dataset box |
| `coordinate/09-nested-comments.png` | [ScreenSpot test row 1020](https://huggingface.co/datasets/bevaya/ScreenSpot) | Apache-2.0 dataset | Source-provided 2560×1440 image; ground-truth box converted from the normalized dataset box |
| `coordinate/10-rating-stars.png` | [ScreenSpot test row 1262](https://huggingface.co/datasets/bevaya/ScreenSpot) | Apache-2.0 dataset | Source-provided 2560×1440 image; ground-truth box converted from the normalized dataset box |
| `ocr/01-vietnamese-street-sign.jpg` | [Nhà Chung street signs](https://commons.wikimedia.org/wiki/File:Nha_Chung_street_signs.jpg) | CC BY 2.0, Nam-ho Park | Wikimedia 1200 px thumbnail |
| `ocr/02-receipt.jpg` | [Sample Product Receipt](https://commons.wikimedia.org/wiki/File:Sample_Product_Receipt.jpg) | CC0 | Original file |
| `ocr/03-multilingual-sign.jpg` | [Nakano Station sign](https://commons.wikimedia.org/wiki/File:A_station_sign_at_Nakano_Station_Tokyo.jpg) | CC0 | Wikimedia 960 px thumbnail |
| `ocr/04-vietnamese-wikipedia.jpg` | [Vietnamese Wikipedia main page](https://commons.wikimedia.org/wiki/File:Screenshot_Vietnamese_Wikipedia_main_page_v1-vi-25.jpg) | CC BY-SA 4.0 | Original file; deterministic 760×270 crop |
| `ocr/05-newspaper.jpg` | [The Sun, January 11, 1920](https://www.loc.gov/resource/sn83030431/1920-01-11/ed-1/?sp=1) | Public-domain historic newspaper via Library of Congress | IIIF 6.25% rendition |
| `ocr/12-near-duplicate-filenames.png` | Screen capture supplied by the maintainer | Own work | Replaced the perspective-diacritics case at difficulty 2, which every endpoint passed at 0.996. Two file names differing only in their timestamp, and short enough for the model to finish, which is the condition under which the upstream Qwen3-VL repetition defect appears; reproduced on this image before it was adopted |
| `ocr/07-status-bar.png` | [ScreenSpot test row 291](https://huggingface.co/datasets/bevaya/ScreenSpot) | Apache-2.0 dataset | Source-provided 2190×1706 image |
| `ocr/08-issue-list.png` | [ScreenSpot test row 850](https://huggingface.co/datasets/bevaya/ScreenSpot) | Apache-2.0 dataset | Source-provided 2560×1440 image |
| `ocr/09-product-grid.png` | [ScreenSpot test row 1250](https://huggingface.co/datasets/bevaya/ScreenSpot) | Apache-2.0 dataset | Source-provided 2560×1440 image |
| `ocr/11-directory-listing.png` | [Dir command in Windows Command Prompt](https://commons.wikimedia.org/wiki/File:Dir_command_in_Windows_Command_Prompt.png) | Public domain via Wikimedia Commons | Replaced the handwriting case at difficulty 10, which every endpoint passed at 0.99. Repeated dates, times and `<DIR>` markers, and two GUID filenames differing by a single character, exercise the upstream repetition defect that the previous corpus could not provoke |
| `localization/01-comment-thread.png` | User-provided screenshot | Private benchmark fixture supplied for this feature diagnosis | Exact copied PNG; no editing or baked overlay |
| `localization/02-mobile-settings.png` | [Commons app Settings screen](https://commons.wikimedia.org/wiki/File:Commons_app_Settings_screen.png) | CC BY-SA 4.0, Misaochan | Original 1080×1920 PNG |
| `localization/03-desktop-audio.png` | [KDE System Settings audio screenshot](https://commons.wikimedia.org/wiki/File:KDE_System_Settings_5.23.90_audio_screenshot.png) | GPL free-software screenshot; screenshot by VulcanSphere | Original 1365×857 PNG |
| `localization/04-vlc-korean.png` | [Korean VLC Media Player screenshot](https://commons.wikimedia.org/wiki/File:VLC_Screenshot_Korean.png) | GPL free-software screenshot; source metadata also records CC BY-SA 3.0 | Original 824×546 PNG |
| `localization/05-mobile-wikipedia-japanese.png` | [Related pages on mobile Japanese Wikipedia](https://commons.wikimedia.org/wiki/File:Screenshot_of_related_pages_on_beta_mobile_Japanese_Wikipedia.png) | CC BY-SA 4.0, Melamrawy (WMF) | Original 640×1136 PNG |
| `localization/06-firefox-arabic.png` | [Arabic Firefox screenshot](https://commons.wikimedia.org/wiki/File:Firefox_Version_141.0.3_Arabic.png) | CC0 1.0, LAnwalt | Original 1920×1017 PNG |

Review source pages before redistributing fixtures outside this repository; they remain the authority for attribution and license details.
