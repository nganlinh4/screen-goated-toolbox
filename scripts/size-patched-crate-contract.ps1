@(
    @{
        Name = "epaint"
        Version = "0.36.1"
        RelativeDirectory = "libs/epaint"
        ArchiveSha256 = "11863a6b2b7823010c1090ddf4706947bdda46766742def0673195cbf7ff4a73"
        PatchedTreeSha256 = "2b468771ae774f90e6c9fac66fa39e23b3214f4087961b51cd3fbc226e0a0e9e"
        Patch = "scripts/epaint-u8-pipeline.patch"
        RequiredText = '"u8_pipeline"'
        ForbiddenText = '"f32_pipeline"'
    }
)
