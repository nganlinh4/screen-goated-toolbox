# ONNX Runtime — JNI and native loader
-keep class com.microsoft.onnxruntime.** { *; }
-keepclassmembers class com.microsoft.onnxruntime.** { *; }

# youtubedl-android — reflection-heavy native bridge
-keep class com.yausername.youtubedl_android.** { *; }
-keep class io.github.junkfood02.youtubedl_android.** { *; }

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
