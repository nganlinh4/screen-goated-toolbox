# ONNX Runtime — JNI and native loader
-keep class ai.onnxruntime.** { *; }
-keepclassmembers class ai.onnxruntime.** { *; }

# Moonshine / Sherpa — their AARs ship no consumer rules, and their JNI resolves
# these classes by string name (FindClass/GetFieldID), which R8 cannot see.
-keep class ai.moonshine.voice.** { *; }
-keepclassmembers class ai.moonshine.voice.** { *; }
-keep class com.k2fsa.sherpa.onnx.** { *; }
-keepclassmembers class com.k2fsa.sherpa.onnx.** { *; }

# youtubedl-android — only initialization state and JSON mapper members are reflective.
# Keeping the entire package would retain its optional runtime self-updater in Play builds.
-keepclassmembers class com.yausername.youtubedl_android.YoutubeDL {
    boolean initialized;
}
-keepclassmembers class com.yausername.ffmpeg.FFmpeg {
    boolean initialized;
}
-keep class com.yausername.youtubedl_android.mapper.** { *; }

# Apache Commons Compress — reflection-based ZIP extra-field registry
# R8 makes AsiExtraField abstract, crashing ExtraFieldUtils.<clinit>
-keep class org.apache.commons.compress.** { *; }
-dontwarn org.tukaani.xz.**

# kotlinx.serialization — keep @Serializable classes
-keepattributes *Annotation*, InnerClasses
-dontnote kotlinx.serialization.AnnotationsKt
-keepclassmembers @kotlinx.serialization.Serializable class ** {
    *** Companion;
}
-keepclasseswithmembers class ** {
    kotlinx.serialization.KSerializer serializer(...);
}
-keep,includedescriptorclasses class dev.screengoated.toolbox.mobile.**$$serializer { *; }
-keepclassmembers class dev.screengoated.toolbox.mobile.** {
    *** Companion;
}
-keepclasseswithmembers class dev.screengoated.toolbox.mobile.** {
    kotlinx.serialization.KSerializer serializer(...);
}

# OkHttp — platform-specific classes
-dontwarn okhttp3.internal.platform.**
-dontwarn org.bouncycastle.**
-dontwarn org.conscrypt.**
-dontwarn org.openjsse.**

# CommonMark — service loader
-keep class org.commonmark.** { *; }
