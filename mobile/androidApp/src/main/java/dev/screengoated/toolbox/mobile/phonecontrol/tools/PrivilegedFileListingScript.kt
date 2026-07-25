package dev.screengoated.toolbox.mobile.phonecontrol.tools

internal val READ_ONLY_LIST_SCRIPT = """
    base=${'$'}1
    [ -d "${'$'}base" ] || exit 44
    for item in "${'$'}base"/* "${'$'}base"/.[!.]* "${'$'}base"/..?*; do
      [ -e "${'$'}item" ] || [ -L "${'$'}item" ] || continue
      if [ -d "${'$'}item" ]; then kind=d; else kind=f; fi
      metadata=$(/system/bin/toybox stat -c '%s:%Y' -- "${'$'}item" 2>/dev/null) || exit 45
      size=${'$'}{metadata%%:*}
      modified=${'$'}{metadata#*:}
      case "${'$'}size:${'$'}modified" in *[!0-9:]*|:*) exit 45 ;; esac
      encoded=$(printf '%s' "${'$'}item" | /system/bin/toybox base64 -w 0) || exit 45
      printf '%s\t%s\t%s\t%s\n' "${'$'}kind" "${'$'}size" "${'$'}modified" "${'$'}encoded"
    done
""".trimIndent()
