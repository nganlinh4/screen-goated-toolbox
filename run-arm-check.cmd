@echo on
set PATH=C:\Program Files\LLVM\bin;%PATH%
call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=arm64 -host_arch=x64
cd /d C:\WORK\screen-goated-toolbox
cargo check --target aarch64-pc-windows-msvc > arm-check.log 2>&1
echo EXITCODE:%ERRORLEVEL%
