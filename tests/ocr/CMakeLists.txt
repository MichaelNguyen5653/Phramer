find_package(Qt${QT_VERSION_MAJOR} CONFIG REQUIRED Core Gui Test)

# The OCR geometry helpers are pure functions with no dependency on the
# application target, so they are compiled straight into the test binary
# rather than requiring src/ to be split into a library.
add_executable(
  ocr_tests
  tst_ocr.cpp
  ${CMAKE_SOURCE_DIR}/src/tools/ocr/ocrpreprocess.cpp
  ${CMAKE_SOURCE_DIR}/src/tools/ocr/ocrlayout.cpp)

set_target_properties(ocr_tests PROPERTIES AUTOMOC ON WIN32_EXECUTABLE OFF)
target_include_directories(ocr_tests PRIVATE ${CMAKE_SOURCE_DIR}/src)
target_link_libraries(
  ocr_tests PRIVATE project_options Qt${QT_VERSION_MAJOR}::Core
                    Qt${QT_VERSION_MAJOR}::Gui Qt${QT_VERSION_MAJOR}::Test)

if(MSVC)
  # Without an explicit console subsystem the test runs but its output goes
  # nowhere, so a failure looks identical to a pass
  target_link_options(ocr_tests PRIVATE /SUBSYSTEM:CONSOLE)
endif()

add_test(NAME ocr_tests COMMAND ocr_tests)

if(WIN32)
  # ctest starts the test with a bare environment, so the Qt DLLs beside
  # Qt6Core have to be put on PATH or it dies with 0xc0000135
  set_tests_properties(
    ocr_tests
    PROPERTIES
      ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt${QT_VERSION_MAJOR}::Core>")
endif()
