@{
    Dependencies = @(
        @{
            Name = "egui-snarl"
            RelativeDirectory = "libs/egui-snarl"
            Repository = "https://github.com/zakarumych/egui-snarl.git"
            Revision = "5bdc34e4ebdb9d7a0968f21564dce51a1a027ee8"
            Shallow = $true
            Patches = @(
                "scripts/egui-snarl-scroll-zoom.patch"
                "scripts/egui-snarl-no-default-fonts.patch"
                "scripts/egui-snarl-rustfmt.patch"
            )
        }
        @{
            Name = "egui-scale"
            RelativeDirectory = "libs/egui-scale"
            Repository = "https://github.com/zakarumych/egui-scale.git"
            Revision = "abb9b647cf9478c6de876a980e4355cdc2d141c8"
            Shallow = $false
            Patches = @(
                "scripts/egui-scale-no-default-fonts.patch"
            )
        }
    )
}
