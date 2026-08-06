cmake_minimum_required(VERSION 3.22)
# cmake_policy(SET CMP0076 OLD)

set(FLAMESHOT_VERSION 14.1.4)

# A release build passes the version derived from the git tag, so the tag
# stays the single source of truth. Keep the plain set() above on one line
# and unadorned: the packaging workflows grep that exact line to read the
# version, and anything trailing (a CACHE clause, a comment) ends up in it.
if(RELEASE_VERSION)
  set(FLAMESHOT_VERSION ${RELEASE_VERSION})
endif()

# An empty or malformed version silently produces project(VERSION ) and hence
# empty FLAMESHOT_VERSION_* defines, which only surface much later as
# "RC2127: version WORDs separated by commas expected" while compiling
# flameshot.rc. Fail here instead, where the cause is obvious.
if(NOT FLAMESHOT_VERSION MATCHES "^[0-9]+\\.[0-9]+\\.[0-9]+$")
  message(
    FATAL_ERROR
      "FLAMESHOT_VERSION must be MAJOR.MINOR.PATCH, got '${FLAMESHOT_VERSION}'")
endif()

# Releases feed this fork checks for updates. It must point at this
# repository: aimed at upstream it would compare against their version
# numbers and send users to their downloads.
#
# Builds up to 14.1.2 shipped with the Flameshot-v2 URL compiled in, so the
# 14.1.3 installer has to be published there as well, one last time, for
# those clients to find it. Everything from 14.1.3 onward asks this repo.
set(GIT_API_URL "https://api.github.com/repos/MichaelNguyen5653/Phramer/releases/latest")

project(
  flameshot
  VERSION ${FLAMESHOT_VERSION}
  LANGUAGES CXX)
set(PROJECT_NAME_CAPITALIZED "Phramer")
# The product ships as Phramer. The CMake target keeps its historical name so
# that the target_sources() calls spread across every directory under src/
# stay valid; only the produced binary and everything user-facing are renamed.
set(APP_BINARY_NAME "phramer")

include(FetchContent)
#Must be set before fetching external content!
#QT_DEFAULT_MAJOR_VERSION used by QHotkey
set(QT_DEFAULT_MAJOR_VERSION 6 CACHE STRING "")
#QT_VERSION_MAJOR used by Flameshot and QtColorWidgets
set(QT_VERSION_MAJOR 6 CACHE STRING "")
#BUILD_SHARED_LIBS used by QHotkey and QtColorWidgets
option(BUILD_SHARED_LIBS OFF)
#QHOTKEY_INSTALL used by QHotkey; must be disabled on Windows
if(WIN32)
set(QHOTKEY_INSTALL OFF CACHE BOOL "qHotkey install")
endif()

#Needed due to linker error with QtColorWidget
set(CMAKE_POSITION_INDEPENDENT_CODE ON)


# Dependency can be fetched via flatpak builder
if(EXISTS "${CMAKE_SOURCE_DIR}/external/Qt-Color-Widgets/CMakeLists.txt")
  add_subdirectory("${CMAKE_SOURCE_DIR}/external/Qt-Color-Widgets" EXCLUDE_FROM_ALL)
else()
  FetchContent_Declare(
    qtColorWidgets
    GIT_REPOSITORY https://gitlab.com/mattbas/Qt-Color-Widgets.git
    GIT_TAG 5d52e907e50dc88cf969b41cea44665ff6c475b1
  )
  #Workaround for duplicate GUID in windows WIX installer
  if(WIN32)
    FetchContent_GetProperties(qtColorWidgets)
    if(NOT qtcolorwidgets_POPULATED)
      FetchContent_Populate(qtColorWidgets)
      add_subdirectory(${qtcolorwidgets_SOURCE_DIR} ${qtcolorwidgets_BINARY_DIR} EXCLUDE_FROM_ALL)
    endif()
  else()
    FetchContent_MakeAvailable(qtColorWidgets)
  endif()
endif()

# This can be read from ${PROJECT_NAME} after project() is called
if(APPLE)
  set(CMAKE_OSX_DEPLOYMENT_TARGET "10.15" CACHE STRING "Minimum OS X deployment version")
endif()


# Configuration options
set(DEFAULT_USE_PORTABLE_CONFIG FALSE)
if(WIN32)
  set(DEFAULT_USE_PORTABLE_CONFIG TRUE)
  # For Windows RC file.
  add_definitions(-DFLAMESHOT_VERSION_MAJOR=${CMAKE_PROJECT_VERSION_MAJOR})
  add_definitions(-DFLAMESHOT_VERSION_MINOR=${CMAKE_PROJECT_VERSION_MINOR})
  add_definitions(-DFLAMESHOT_VERSION_BUGFIX=${CMAKE_PROJECT_VERSION_PATCH})
  add_definitions(-DFLAMESHOT_VERSION_BUILD=1)
  add_definitions(-DFLAMESHOT_VERSION_STRING="${PROJECT_VERSION}")
elseif(APPLE)
  set(Qt6_DIR "$(brew --prefix qt6)/lib/cmake/Qt6/" CACHE PATH "directory where Qt6Config.cmake exists.")
endif()
set(USE_PORTABLE_CONFIG
  ${DEFAULT_USE_PORTABLE_CONFIG}
  CACHE BOOL "Store config in application folder")
if(USE_PORTABLE_CONFIG)
add_compile_definitions(USE_PORTABLE_CONFIG)
endif()

option(FLAMESHOT_DEBUG_CAPTURE "Enable mode to make debugging easier" OFF)
option(USE_MONOCHROME_ICON "Build using monochrome icon as default" OFF)
option(GENERATE_TS "Regenerate translation source files" OFF)
option(USE_KDSINGLEAPPLICATION "Use KDSingleApplication library" ON)
option(USE_BUNDLED_KDSINGLEAPPLICATION "Use a bundled version of the KDSingleApplication library" ${USE_KDSINGLEAPPLICATION})
option(USE_LAUNCHER_ABSOLUTE_PATH "Use absolute path for the desktop launcher" ON)
option(USE_WAYLAND_CLIPBOARD "USE KF Gui Wayland Clipboard" OFF)
option(DISABLE_UPDATE_CHECKER "Disable check for updates" OFF)
option(ENABLE_IMGUR "Enable Imgur Uploader" OFF)

if(ENABLE_IMGUR)
  add_compile_definitions(ENABLE_IMGUR)
endif()

if(DISABLE_UPDATE_CHECKER)
  add_compile_definitions(DISABLE_UPDATE_CHECKER)
endif()

include(cmake/StandardProjectSettings.cmake)

add_library(project_options INTERFACE)
target_compile_features(project_options INTERFACE cxx_std_20)

add_library(project_warnings INTERFACE)

# enable cache system
include(cmake/Cache.cmake)

# standard compiler warnings
include(cmake/CompilerWarnings.cmake)
# set_project_warnings(project_warnings)

# sanitizer options if supported by compiler
include(cmake/Sanitizers.cmake)
enable_sanitizers(project_options)

if(USE_KDSINGLEAPPLICATION)
  if(USE_BUNDLED_KDSINGLEAPPLICATION)
    set(KDSingleApplication_EXAMPLES OFF CACHE BOOL "Don't build the examples")
    set(KDSingleApplication_STATIC ON CACHE BOOL "Build static versions of the libraries")

    # Check if KDSingleApplication is available locally
    if(EXISTS "${CMAKE_SOURCE_DIR}/external/KDSingleApplication/CMakeLists.txt")
      add_subdirectory("${CMAKE_SOURCE_DIR}/external/KDSingleApplication")
    else()
      FetchContent_Declare(
        kdsingleApplication
        GIT_REPOSITORY https://github.com/KDAB/KDSingleApplication.git
        GIT_TAG v1.2.1
      )
      FetchContent_MakeAvailable(kdsingleApplication)
    endif()
  else()
    find_package(KDSingleApplication-qt6 REQUIRED)
  endif()
endif()

# allow for static analysis options (include after kdsingleApplication because
# of potential conflict if ENABLE_QT_DEPRECATION_CHECK is used)
include(cmake/StaticAnalyzers.cmake)

# ToDo: Check if this is used anywhere
option(BUILD_STATIC_LIBS ON)

if(WIN32 OR APPLE)
  FetchContent_Declare(
    qHotKey
    GIT_REPOSITORY https://github.com/flameshot-org/QHotkey
    GIT_TAG master
  )
  FetchContent_MakeAvailable(QHotKey)
endif()

add_subdirectory(src)

# Off by default so the packaging workflows and CPack never see this target.
# Configure with -DBUILD_OCR_TESTS=ON to build and run it.
option(BUILD_OCR_TESTS "Build the OCR geometry unit tests" OFF)
if(BUILD_OCR_TESTS)
  enable_testing()
  add_subdirectory(tests/ocr)
endif()

# CPack
set(CPACK_PACKAGE_VENDOR "Phramer")
set(CPACK_PACKAGE_DESCRIPTION_SUMMARY "Powerful yet simple to use screenshot software.")
set(CPACK_PACKAGE_VERSION ${PROJECT_VERSION})
set(CPACK_PACKAGE_HOMEPAGE_URL "https://github.com/MichaelNguyen5653/Phramer")
set(CPACK_PACKAGE_CONTACT "Phramer")
set(CPACK_PACKAGE_ICON "${CMAKE_SOURCE_DIR}/data/img/app/appicon-256.png") # TODO: Can any generator make
# use of this?
set(CPACK_PACKAGE_DESCRIPTION_FILE "${CMAKE_SOURCE_DIR}/README.md") # TODO: Where is this used? Do we need a better
# source?

if(WIN32)
  # Include all dynamically linked runtime libraries such as MSVCRxxx.dll
  include(InstallRequiredSystemLibraries)

  if(USE_PORTABLE_CONFIG)
    set(CPACK_PACKAGE_FILE_NAME "${APP_BINARY_NAME}-${PROJECT_VERSION}-win64")
    set(CPACK_GENERATOR ZIP)

  else()
    set(CPACK_GENERATOR WIX ZIP)
    set(CPACK_PACKAGE_NAME "Phramer")
    set(CPACK_PACKAGE_INSTALL_DIRECTORY "Phramer")
    set(CPACK_PACKAGE_EXECUTABLES ${APP_BINARY_NAME} "${PROJECT_NAME_CAPITALIZED}")
    set(CPACK_CREATE_DESKTOP_LINKS ${APP_BINARY_NAME})

    # WIX (Windows .msi installer)
    # 48x48 pixels
    set(CPACK_WIX_PRODUCT_ICON "${CMAKE_SOURCE_DIR}/data/img/app/appicon.ico")
    # Supported languages can be found at http://wixtoolset.org/documentation/manual/v3/wixui/wixui_localization.html
    # set(CPACK_WIX_CULTURES "ar-SA,bg-BG,ca-ES,hr-HR,cs-CZ,da-DK,nl-NL,en-US,et-EE,fi-FI,fr-FR,de-DE")
    set(CPACK_WIX_UI_BANNER "${CMAKE_SOURCE_DIR}/packaging/win-installer/Bitmaps/CPACK_WIX_UI_BANNER.BMP")
    set(CPACK_WIX_UI_DIALOG "${CMAKE_SOURCE_DIR}/packaging/win-installer/Bitmaps/CPACK_WIX_UI_DIALOG.BMP")
    set(CPACK_WIX_PROPERTY_ARPHELPLINK "${CPACK_PACKAGE_HOMEPAGE_URL}")
    set(CPACK_WIX_PROPERTY_ARPURLINFOABOUT "${CPACK_PACKAGE_HOMEPAGE_URL}")
    set(CPACK_WIX_ROOT_FEATURE_DESCRIPTION "${CPACK_PACKAGE_DESCRIPTION_SUMMARY}")
    set(CPACK_WIX_LIGHT_EXTRA_FLAGS "-dcl:high") # set high compression

    set(CPACK_RESOURCE_FILE_LICENSE "${CMAKE_SOURCE_DIR}/packaging/win-installer/LICENSE/GPL-3.0.txt")
    set(CPACK_RESOURCE_FILE_README "${CMAKE_SOURCE_DIR}/README.md")

    # Unchanged through the rename to Phramer: this GUID is what makes
    # msiexec upgrade an existing install in place rather than leaving two
    # entries in Add/Remove Programs. It is distinct from upstream
    # Flameshot's, so it never touches an upstream installation.
    set(CPACK_WIX_UPGRADE_GUID "42607922-E95D-4601-A561-F750286F20B6")
  endif()
elseif(APPLE)
  set(CPACK_INCLUDE_TOPLEVEL_DIRECTORY 0)
  set(CPACK_PACKAGE_FILE_NAME "${APP_BINARY_NAME}-${PROJECT_VERSION}-osx")
  set(CPACK_GENERATOR ZIP)
else()
  set(CPACK_PACKAGE_FILE_NAME "${APP_BINARY_NAME}-${PROJECT_VERSION}-linux")
  set(CPACK_GENERATOR TGZ)
  set(CPACK_SOURCE_GENERATOR TGZ)
endif()

include(CPack)
