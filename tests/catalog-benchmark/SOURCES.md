# Catalog benchmark fixture sources

The benchmark keeps the downloaded source image unchanged apart from source-provided thumbnail scaling. Ground-truth overlays live in `review.html`; they are not baked into model inputs. OCR cases 3 and 4 define deterministic runtime crops in `manifest.json`; no generative image editing is used.

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
| `ocr/01-poster.jpg` | [The Right to Read poster](https://commons.wikimedia.org/wiki/File:The_Right_to_Read_poster,_1970.jpg) | United States government work, public domain | Wikimedia 500 px thumbnail |
| `ocr/02-receipt.jpg` | [Sample Product Receipt](https://commons.wikimedia.org/wiki/File:Sample_Product_Receipt.jpg) | CC0 | Original file |
| `ocr/03-multilingual-sign.jpg` | [Nakano Station sign](https://commons.wikimedia.org/wiki/File:A_station_sign_at_Nakano_Station_Tokyo.jpg) | CC0 | Wikimedia 960 px thumbnail |
| `ocr/04-historic-menu.jpg` | [Pulaski House restaurant menu](https://commons.wikimedia.org/wiki/File:Pulaski_House_restaurant_menu_(April_20,_1857).jpg) | Public domain / freely licensed historic scan | Original file |
| `ocr/05-newspaper.jpg` | [The Sun, January 11, 1920](https://www.loc.gov/resource/sn83030431/1920-01-11/ed-1/?sp=1) | Public-domain historic newspaper via Library of Congress | IIIF 6.25% rendition |
| `ocr/06-perspective-sign.jpg` | [Street sign 01](https://commons.wikimedia.org/wiki/File:Street_sign_01.JPG) | CC0 | Wikimedia 1200 px thumbnail |
| `ocr/07-status-bar.png` | [ScreenSpot test row 291](https://huggingface.co/datasets/bevaya/ScreenSpot) | Apache-2.0 dataset | Source-provided 2190×1706 image |
| `ocr/08-issue-list.png` | [ScreenSpot test row 850](https://huggingface.co/datasets/bevaya/ScreenSpot) | Apache-2.0 dataset | Source-provided 2560×1440 image |
| `ocr/09-product-grid.png` | [ScreenSpot test row 1250](https://huggingface.co/datasets/bevaya/ScreenSpot) | Apache-2.0 dataset | Source-provided 2560×1440 image |
| `ocr/10-handwritten-address.jpg` | [Gettysburg Address, Nicolay Copy, page 1](https://www.loc.gov/exhibits/gettysburg-address/exhibition-items.html#obj4) | Public-domain 1863 manuscript via Library of Congress | LOC enlarged image; reference follows the LOC Nicolay transcription |

Review source pages before redistributing fixtures outside this repository; they remain the authority for attribution and license details.
