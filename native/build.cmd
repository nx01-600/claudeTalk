@echo off
rem Builds the native crates that compile C++ (whisper.cpp): MSVC env,
rem Ninja instead of MSBuild (MSBuild trips over 260-char paths in the
rem Vulkan shader build), a short target dir, LLVM for bindgen, Vulkan SDK.
rem   native\build.cmd <dir> cargo build --release
setlocal
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
for /f "usebackq delims=" %%i in (`"%VSWHERE%" -latest -products * -property installationPath`) do set "VS=%%i"
call "%VS%\VC\Auxiliary\Build\vcvars64.bat" >nul || exit /b 1
set "CMAKE_GENERATOR=Ninja"
for /d %%n in ("%LOCALAPPDATA%\Microsoft\WinGet\Packages\Ninja-build.Ninja*") do set "PATH=%%n;%PATH%"
set "PATH=%PATH%;%ProgramFiles%\CMake\bin;%ProgramFiles%\LLVM\bin;%LOCALAPPDATA%\Microsoft\WinGet\Links"
if not defined LIBCLANG_PATH set "LIBCLANG_PATH=%ProgramFiles%\LLVM\bin"
if not defined VULKAN_SDK for /d %%v in (C:\VulkanSDK\*) do set "VULKAN_SDK=%%v"
if not defined CARGO_TARGET_DIR set "CARGO_TARGET_DIR=C:\ctb"
pushd "%~dp0%1" || exit /b 1
shift
set ARGS=
:collect
if "%~1"=="" goto run
set ARGS=%ARGS% %1
shift
goto collect
:run
%ARGS%
set RC=%ERRORLEVEL%
popd
exit /b %RC%
