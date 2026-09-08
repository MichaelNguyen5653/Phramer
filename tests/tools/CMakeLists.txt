find_package(Qt${QT_VERSION_MAJOR} CONFIG REQUIRED Core Gui Widgets Test)

# The two-point and path bases only reach as far as CaptureContext, so they
# compile straight into the test binary rather than requiring src/ to be
# split into a library.
add_executable(
  tool_tests
  tst_toolmove.cpp
  # capturetool.h declares Q_OBJECT but has no matching .cpp, so AUTOMOC
  # only sees it when it is listed as a source in its own right
  ${CMAKE_SOURCE_DIR}/src/tools/capturetool.h
  ${CMAKE_SOURCE_DIR}/src/tools/abstractpathtool.cpp
  ${CMAKE_SOURCE_DIR}/src/tools/abstracttwopointtool.cpp
  ${CMAKE_SOURCE_DIR}/src/tools/capturecontext.cpp
  ${CMAKE_SOURCE_DIR}/src/core/capturerequest.cpp)

set_target_properties(tool_tests PROPERTIES AUTOMOC ON WIN32_EXECUTABLE OFF)
target_include_directories(tool_tests PRIVATE ${CMAKE_SOURCE_DIR}/src)
target_link_libraries(
  tool_tests PRIVATE project_options Qt${QT_VERSION_MAJOR}::Core
                     Qt${QT_VERSION_MAJOR}::Gui Qt${QT_VERSION_MAJOR}::Test)

if(MSVC)
  # Without an explicit console subsystem the test runs but its output goes
  # nowhere, so a failure looks identical to a pass
  target_link_options(tool_tests PRIVATE /SUBSYSTEM:CONSOLE)
endif()

add_test(NAME tool_tests COMMAND tool_tests)

if(WIN32)
  # ctest starts the test with a bare environment, so the Qt DLLs beside
  # Qt6Core have to be put on PATH or it dies with 0xc0000135
  set_tests_properties(
    tool_tests
    PROPERTIES
      ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt${QT_VERSION_MAJOR}::Core>")
endif()

# Picking is exercised through CaptureToolObjects, which only reaches as far
# as the CaptureTool interface. TextTool itself is not linked here: it pulls
# in ConfigHandler, which reaches ValueHandler and from there every tool.
add_executable(
  tool_picking_tests
  tst_toolpicking.cpp
  ${CMAKE_SOURCE_DIR}/src/tools/capturetool.h
  ${CMAKE_SOURCE_DIR}/src/widgets/capture/capturetoolobjects.cpp
  ${CMAKE_SOURCE_DIR}/src/tools/capturecontext.cpp
  ${CMAKE_SOURCE_DIR}/src/core/capturerequest.cpp)

set_target_properties(tool_picking_tests PROPERTIES AUTOMOC ON
                                                    WIN32_EXECUTABLE OFF)
target_include_directories(tool_picking_tests PRIVATE ${CMAKE_SOURCE_DIR}/src)
target_link_libraries(
  tool_picking_tests
  PRIVATE project_options Qt${QT_VERSION_MAJOR}::Core Qt${QT_VERSION_MAJOR}::Gui
          Qt${QT_VERSION_MAJOR}::Widgets Qt${QT_VERSION_MAJOR}::Test)

if(MSVC)
  target_link_options(tool_picking_tests PRIVATE /SUBSYSTEM:CONSOLE)
endif()

add_test(NAME tool_picking_tests COMMAND tool_picking_tests)

if(WIN32)
  set_tests_properties(
    tool_picking_tests
    PROPERTIES
      ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt${QT_VERSION_MAJOR}::Core>")
endif()
