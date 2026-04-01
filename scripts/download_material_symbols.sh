#!/bin/bash
# Download Material Symbols as Android vector drawables from Iconify API
# Converts SVG path data into Android VectorDrawable XML format

DRAWABLE_DIR="mobile/androidApp/src/main/res/drawable"
mkdir -p "$DRAWABLE_DIR"

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
  ["close"]="close"
  ["computer"]="computer"
  ["content_copy"]="content_copy"
  ["content_cut"]="content_cut"
  ["content_paste"]="content_paste"
  ["delete"]="delete"
  ["description"]="description"
  ["download"]="download"
  ["drag_indicator"]="drag_indicator"
  ["edit"]="edit"
  ["expand_less"]="expand_less"
  ["expand_more"]="expand_more"
  ["fiber_smart_record"]="fiber_smart_record"
  ["folder"]="folder"
  ["format_quote"]="format_quote"
  ["gamepad"]="gamepad"
  ["graphic_eq"]="graphic_eq"
  ["grid_view"]="grid_view"
  ["g_translate"]="g_translate"
  ["help_outline"]="help_outline"
  ["history"]="history"
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
  ["note"]="note"
  ["open_in_full"]="open_in_full"
  ["open_in_new"]="open_in_new"
  ["photo_camera"]="photo_camera"
  ["play_arrow"]="play_arrow"
  ["public"]="public"
  ["qr_code_scanner"]="qr_code_scanner"
  ["question_answer"]="question_answer"
  ["record_voice_over"]="record_voice_over"
  ["refresh"]="refresh"
  ["remove"]="remove"
  ["remove_red_eye"]="remove_red_eye"
  ["restart_alt"]="restart_alt"
  ["school"]="school"
  ["search"]="search"
  ["send"]="send"
  ["settings"]="settings"
  ["settings_voice"]="settings_voice"
  ["smart_toy"]="smart_toy"
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
  ["voice_chat"]="voice_chat"
  ["volume_off"]="volume_off"
  ["volume_up"]="volume_up"
)

count=0
total=${#ICONS[@]}

for key in "${!ICONS[@]}"; do
  symbol="${ICONS[$key]}"
  outfile="$DRAWABLE_DIR/ms_${key}.xml"

  if [ -f "$outfile" ]; then
    echo "SKIP $key (exists)"
    continue
  fi

  # Download SVG from Iconify
  svg=$(curl -s "https://api.iconify.design/material-symbols:${symbol}-rounded.svg" 2>/dev/null)

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

  count=$((count + 1))
  echo "OK   $key ($count/$total)"
done

echo ""
echo "Downloaded $count icons to $DRAWABLE_DIR"
