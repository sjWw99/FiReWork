@echo off
setlocal
echo Building FiReWork.exe (Rust release)...
cargo build --release
if errorlevel 1 (
  echo BUILD FAILED.
  exit /b 1
)
copy /y target\release\FiReWork.exe FiReWork.exe >nul
echo Done: FiReWork.exe
endlocal
