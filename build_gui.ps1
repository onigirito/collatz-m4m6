$env:PATH = "C:\msys64\mingw64\bin;C:\Users\ykihi\.cargo\bin;" + $env:PATH
Set-Location "C:\Users\ykihi\Desktop\collatz-m4m6"
cargo build --release --features gui --bin collatz-gui
