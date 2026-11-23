#!/bin/bash
# Download filled, rounded Material Symbols from Iconify for Android and offline web UIs.
# Converts SVG path data into Android VectorDrawable XML and keeps the source SVG.
#
# Usage: download_material_symbols.sh [name ...]   (no names: every icon below)

# grep -P below needs a UTF-8 locale (Git Bash defaults to none).
export LC_ALL=C.UTF-8

DRAWABLE_DIR="mobile/androidApp/src/main/res/drawable"
WEB_ICON_DIR="ui-shared/material-symbols"
ANDROID_WEB_ICON_DIR="mobile/androidApp/src/main/assets/material-symbols"
mkdir -p "$DRAWABLE_DIR"
mkdir -p "$WEB_ICON_DIR"
mkdir -p "$ANDROID_WEB_ICON_DIR"

# Map: compose_name -> material_symbols_name
declare -A ICONS=(
  ["account_tree"]="account_tree"
  ["add"]="add"
  ["apps"]="apps"
  ["arrow_back"]="arrow_back"
  ["arrow_drop_down"]="arrow_drop_down"
  ["arrow_forward"]="arrow_forward"
  ["auto_awesome"]="auto_awesome"
  ["auto_fix_high"]="auto_fix_high"
  ["bar_chart"]="bar_chart"
  ["bolt"]="bolt"
  ["breakfast_dining"]="breakfast_dining"
  ["camera_alt"]="photo_camera"
  ["check"]="check"
  ["cleaning_services"]="cleaning_services"
  ["close"]="close"
  ["compare_arrows"]="compare_arrows"
  ["computer"]="computer"
  ["content_copy"]="content_copy"
  ["content_cut"]="content_cut"
  ["content_paste"]="content_paste"
  ["delete"]="delete"
  ["deployed_code"]="deployed-code"
  ["description"]="description"
  ["dictionary"]="dictionary"
  ["download"]="download"
  ["draw_collage"]="draw-collage"
  ["drag_indicator"]="drag_indicator"
  ["edit"]="edit"
  ["expand_less"]="expand_less"
  ["expand_more"]="expand_more"
  ["fiber_smart_record"]="fiber_smart_record"
  ["file_copy"]="file_copy"
  ["file_copy_off"]="file_copy_off"
  ["filter"]="filter"
  ["folder"]="folder"
  ["format_italic"]="format_italic"
  ["format_quote"]="format_quote"
  ["gamepad"]="gamepad"
  ["graphic_eq"]="graphic_eq"
  ["grid_view"]="grid_view"
  ["grid_on"]="grid_on"
  ["g_translate"]="g_translate"
  ["help_outline"]="help_outline"
  ["history"]="history"
  ["hourglass_empty"]="hourglass_empty"
  ["image"]="image"
  ["image_search"]="image_search"
  ["info"]="info"
  ["key"]="key"
  ["keyboard"]="keyboard"
  ["keyboard_arrow_down"]="keyboard_arrow_down"
  ["keyboard_arrow_up"]="keyboard_arrow_up"
  ["language"]="language"
  ["lightbulb"]="lightbulb"
  ["local_fire_department"]="local_fire_department"
  ["mic"]="mic"
  ["music_note"]="music_note"
  ["newsmode"]="newsmode"
  ["note"]="note"
  ["notes"]="notes"
  ["opacity"]="opacity"
  ["open_in_full"]="open_in_full"
  ["open_in_new"]="open_in_new"
  ["outline"]="border_style"
  ["pause"]="pause"
  ["palette"]="palette"
  ["photo_camera"]="photo_camera"
  ["play_arrow"]="play_arrow"
  ["potted_plant"]="potted_plant"
  ["public"]="public"
  ["qr_code_scanner"]="qr_code_scanner"
  ["question_answer"]="question_answer"
  ["record_voice_over"]="record_voice_over"
  ["refresh"]="refresh"
  ["remove"]="remove"
  ["remove_red_eye"]="remove_red_eye"
  ["restart_alt"]="restart_alt"
  ["rotate_360"]="360"
  ["school"]="school"
  ["search"]="search"
  ["send"]="send"
  ["settings"]="settings"
  ["settings_voice"]="settings_voice"
  ["smart_toy"]="smart_toy"
  ["speaker_group"]="speaker_group"
  ["speaker_phone"]="speaker_phone"
  ["spellcheck"]="spellcheck"
  ["star"]="star"
  ["star_outline"]="star_outline"
  ["stop"]="stop"
  ["subtitles"]="subtitles"
  ["summarize"]="summarize"
  ["swap_horiz"]="swap_horiz"
  ["system_update"]="system_update"
  ["table_chart"]="table_chart"
  ["text_fields"]="text_fields"
  ["text_snippet"]="text_snippet"
  ["translate"]="translate"
  ["verified"]="verified"
  ["videocam"]="videocam"
  ["visibility"]="visibility"
  ["visibility_off"]="visibility_off"
  ["view_in_ar"]="view_in_ar"
  ["voice_chat"]="voice_chat"
  ["volume_off"]="volume_off"
  ["volume_up"]="volume_up"
  ["fit_screen"]="fit_screen"
  ["redo"]="redo"
  ["save"]="save"
  ["select_all"]="select_all"
  ["texture"]="texture"
  ["undo"]="undo"
  ["zoom_in"]="zoom_in"
  ["zoom_out"]="zoom_out"
  ["emoji_objects"]="emoji_objects"
  ["location_searching"]="location_searching"
  ["my_location"]="my_location"
  ["person_celebrate"]="person_celebrate"
  ["keyboard_double_arrow_left"]="keyboard_double_arrow_left"
  ["keyboard_double_arrow_right"]="keyboard_double_arrow_right"
  ["keyboard_double_arrow_up"]="keyboard_double_arrow_up"
  ["table_eye"]="table_eye"
  ["hov"]="hov"
  ["graph_1"]="graph_1"
  ["graph_2"]="graph_2"
  ["graph_3"]="graph_3"
  ["graph_4"]="graph_4"
  ["graph_5"]="graph_5"
  ["graph_6"]="graph_6"
  ["graph_7"]="graph_7"
  ["hourglass_bottom"]="hourglass_bottom"
  ["hourglass_top"]="hourglass_top"
  ["cognition"]="cognition"
  ["bug_report"]="bug_report"
  ["frame_bug"]="frame_bug"
  ["eyeglasses_2"]="eyeglasses_2"
  ["eyeglasses_3"]="eyeglasses_3"
  ["sentiment_calm"]="sentiment_calm"
  ["sentiment_content"]="sentiment_content"
  ["sentiment_dissatisfied"]="sentiment_dissatisfied"
  ["sentiment_extremely_dissatisfied"]="sentiment_extremely_dissatisfied"
  ["sentiment_neutral"]="sentiment_neutral"
  ["sentiment_sad"]="sentiment_sad"
  ["sentiment_satisfied"]="sentiment_satisfied"
  ["sentiment_stressed"]="sentiment_stressed"
  ["sentiment_very_dissatisfied"]="sentiment_very_dissatisfied"
  ["sentiment_very_satisfied"]="sentiment_very_satisfied"
  ["sentiment_worried"]="sentiment_worried"
  ["sentiment_excited"]="sentiment_excited"
  ["sentiment_frustrated"]="sentiment_frustrated"
  ["arrow_selector_tool"]="arrow_selector_tool"
  ["rtt"]="rtt"
  ["gesture"]="gesture"
  ["keyboard_double_arrow_down"]="keyboard_double_arrow_down"
  ["backlight_high"]="backlight_high"
  ["globe_asia"]="globe_asia"
  ["person_play"]="person_play"
  ["terminal"]="terminal"
  ["comment"]="comment"
  ["check_small"]="check_small"
)

count=0
total=${#ICONS[@]}
declare -A REQUESTED=()
for requested in "$@"; do
  REQUESTED["$requested"]=1
done

for key in "${!ICONS[@]}"; do
  if [ "$#" -gt 0 ] && [ -z "${REQUESTED[$key]+x}" ]; then
    continue
  fi
  symbol="${ICONS[$key]}"
  symbol_url="${symbol//_/-}"
  outfile="$DRAWABLE_DIR/ms_${key}.xml"
  webfile="$WEB_ICON_DIR/${key}.svg"
  android_webfile="$ANDROID_WEB_ICON_DIR/${key}.svg"

  if [ -f "$outfile" ] && [ -f "$webfile" ] && [ -f "$android_webfile" ]; then
    echo "SKIP $key (exists)"
    continue
  fi

  # Download SVG from Iconify
  svg=$(curl -s "https://api.iconify.design/material-symbols/${symbol_url}-rounded.svg" 2>/dev/null)

  if [ -z "$svg" ] || echo "$svg" | grep -q "404"; then
    echo "FAIL $key ($symbol) — not found"
    continue
  fi

  # Extract path data from SVG
  path_data=$(echo "$svg" | grep -oP 'd="[^"]*"' | sed 's/d="//;s/"//')

  if [ -z "$path_data" ]; then
    echo "FAIL $key — no path data"
    continue
  fi

  if [ ! -f "$outfile" ]; then
    # Write Android VectorDrawable XML
    cat > "$outfile" << EOF
<vector xmlns:android="http://schemas.android.com/apk/res/android"
    android:width="24dp"
    android:height="24dp"
    android:viewportWidth="24"
    android:viewportHeight="24">
  <path
      android:fillColor="@android:color/white"
      android:pathData="$path_data"/>
</vector>
EOF
  fi

  if [ ! -f "$webfile" ]; then
    printf '%s\n' "$svg" > "$webfile"
  fi

  if [ ! -f "$android_webfile" ]; then
    printf '%s\n' "$svg" > "$android_webfile"
  fi

  count=$((count + 1))
  echo "OK   $key ($count/$total)"
done

echo ""
echo "Downloaded $count icons to $DRAWABLE_DIR, $WEB_ICON_DIR, and $ANDROID_WEB_ICON_DIR"
