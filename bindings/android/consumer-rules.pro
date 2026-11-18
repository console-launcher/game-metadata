# The JNI symbols name gamemeta.GameMeta and its native methods: keep both from renaming.
-keep class gamemeta.GameMeta { native <methods>; }
-keepclasseswithmembernames,includedescriptorclasses class gamemeta.** { native <methods>; }
# The native side calls these callback interfaces by method name (JNI), so their methods must keep their names.
-keep interface gamemeta.TitleFiles { *; }
-keep interface gamemeta.FileOpener { *; }
-keep interface gamemeta.FileSource { *; }
