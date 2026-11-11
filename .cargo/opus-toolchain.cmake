# audiopus_sys searches <out>/lib, but GNUInstallDirs picks lib64 on Fedora-like systems.
set(CMAKE_INSTALL_LIBDIR lib CACHE PATH "" FORCE)
