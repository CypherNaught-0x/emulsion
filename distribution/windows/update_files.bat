
@echo off
set "distribution_dir=%cd%"
cd ..\..
cargo build --release --features=networking

cd %distribution_dir%
mkdir program
copy /y ..\..\target\release\emulsion.exe program\emulsion.exe

