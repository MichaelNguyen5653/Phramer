find_package(Qt${QT_VERSION_MAJOR} CONFIG REQUIRED Core Gui Test)

# Both helpers are free functions over plain values, so each test links exactly
# the one source it exercises and needs no QApplication or widget tree.
add_executable(canvas_geometry_tests tst_canvasgeometry.cpp
                                     ${CMAKE_SOURCE_DIR}/src/utils/canvasgeometry.cpp)

set_target_properties(canvas_geometry_tests PROPERTIES AUTOMOC ON
                                                       WIN32_EXECUTABLE OFF)
target_include_directories(canvas_geometry_tests PRIVATE ${CMAKE_SOURCE_DIR}/src)
target_link_libraries(
  canvas_geometry_tests
  PRIVATE project_options Qt${QT_VERSION_MAJOR}::Core Qt${QT_VERSION_MAJOR}::Gui
          Qt${QT_VERSION_MAJOR}::Test)

if(MSVC)
  # Without an explicit console subsystem the test runs but its output goes
  # nowhere, so a failure looks identical to a pass
  target_link_options(canvas_geometry_tests PRIVATE /SUBSYSTEM:CONSOLE)
endif()

add_test(NAME canvas_geometry_tests COMMAND canvas_geometry_tests)

add_executable(hint_placement_tests tst_hintplacement.cpp
                                    ${CMAKE_SOURCE_DIR}/src/utils/hintplacement.cpp)

set_target_properties(hint_placement_tests PROPERTIES AUTOMOC ON
                                                      WIN32_EXECUTABLE OFF)
target_include_directories(hint_placement_tests PRIVATE ${CMAKE_SOURCE_DIR}/src)
target_link_libraries(
  hint_placement_tests
  PRIVATE project_options Qt${QT_VERSION_MAJOR}::Core Qt${QT_VERSION_MAJOR}::Gui
          Qt${QT_VERSION_MAJOR}::Test)

if(MSVC)
  target_link_options(hint_placement_tests PRIVATE /SUBSYSTEM:CONSOLE)
endif()

add_test(NAME hint_placement_tests COMMAND hint_placement_tests)

if(WIN32)
  # ctest starts the test with a bare environment, so the Qt DLLs beside
  # Qt6Core have to be put on PATH or it dies with 0xc0000135
  set_tests_properties(
    canvas_geometry_tests hint_placement_tests
    PROPERTIES
      ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt${QT_VERSION_MAJOR}::Core>")
endif()
