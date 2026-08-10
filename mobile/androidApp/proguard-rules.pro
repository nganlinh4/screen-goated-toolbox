# Moonshine / Sherpa — their AARs ship no consumer rules, and their JNI resolves
# these classes by string name (FindClass/GetFieldID), which R8 cannot see.
-keep class ai.moonshine.voice.** { *; }
-keepclassmembers class ai.moonshine.voice.** { *; }
-keep class com.k2fsa.sherpa.onnx.** { *; }
-keepclassmembers class com.k2fsa.sherpa.onnx.** { *; }

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
-dontwarn org.openjsse.**

# CommonMark — service loader
-keep class org.commonmark.** { *; }
