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

# AcceptTool reaches no further than CaptureContext. The other export tools
# are not linked here: AppLauncher needs AppLauncherWidget, which pulls in
# ConfigHandler and from there every tool.
add_executable(
  export_signal_tests
  tst_exportsignals.cpp
  ${CMAKE_SOURCE_DIR}/src/tools/capturetool.h
  ${CMAKE_SOURCE_DIR}/src/tools/accept/accepttool.cpp
  ${CMAKE_SOURCE_DIR}/src/tools/abstractactiontool.cpp
  ${CMAKE_SOURCE_DIR}/src/tools/capturecontext.cpp
  ${CMAKE_SOURCE_DIR}/src/core/capturerequest.cpp
  # Leaf helpers AcceptTool::icon() resolves against
  ${CMAKE_SOURCE_DIR}/src/utils/colorutils.cpp
  ${CMAKE_SOURCE_DIR}/src/utils/pathinfo.cpp)

set_target_properties(export_signal_tests PROPERTIES AUTOMOC ON
                                                     WIN32_EXECUTABLE OFF)
target_include_directories(export_signal_tests PRIVATE ${CMAKE_SOURCE_DIR}/src)
target_link_libraries(
  export_signal_tests
  PRIVATE project_options Qt${QT_VERSION_MAJOR}::Core Qt${QT_VERSION_MAJOR}::Gui
          Qt${QT_VERSION_MAJOR}::Widgets Qt${QT_VERSION_MAJOR}::Test)

if(MSVC)
  target_link_options(export_signal_tests PRIVATE /SUBSYSTEM:CONSOLE)
endif()

add_test(NAME export_signal_tests COMMAND export_signal_tests)

if(WIN32)
  set_tests_properties(
    export_signal_tests
    PROPERTIES
      ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt${QT_VERSION_MAJOR}::Core>")
endif()

# Pure arithmetic with no Qt object dependencies, so it needs no QApplication
add_executable(
  wheel_tests
  tst_toolsizewheel.cpp
  ${CMAKE_SOURCE_DIR}/src/utils/toolsizewheel.cpp)

set_target_properties(wheel_tests PROPERTIES AUTOMOC ON WIN32_EXECUTABLE OFF)
target_include_directories(wheel_tests PRIVATE ${CMAKE_SOURCE_DIR}/src)
target_link_libraries(
  wheel_tests PRIVATE project_options Qt${QT_VERSION_MAJOR}::Core
                      Qt${QT_VERSION_MAJOR}::Test)

if(MSVC)
  target_link_options(wheel_tests PRIVATE /SUBSYSTEM:CONSOLE)
endif()

add_test(NAME wheel_tests COMMAND wheel_tests)

if(WIN32)
  set_tests_properties(
    wheel_tests
    PROPERTIES
      ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt${QT_VERSION_MAJOR}::Core>")
endif()

# The fill is a free function over a pixmap, so only its own source is linked.
# QPixmap needs a QGuiApplication, which QTEST_MAIN provides with Gui linked.
add_executable(
  frosted_fill_tests
  tst_frostedfill.cpp
  ${CMAKE_SOURCE_DIR}/src/tools/pixelate/frostedfill.cpp)

set_target_properties(frosted_fill_tests PROPERTIES AUTOMOC ON
                                                    WIN32_EXECUTABLE OFF)
target_include_directories(frosted_fill_tests PRIVATE ${CMAKE_SOURCE_DIR}/src)
target_link_libraries(
  frosted_fill_tests
  PRIVATE project_options Qt${QT_VERSION_MAJOR}::Core Qt${QT_VERSION_MAJOR}::Gui
          Qt${QT_VERSION_MAJOR}::Test)

if(MSVC)
  target_link_options(frosted_fill_tests PRIVATE /SUBSYSTEM:CONSOLE)
endif()

add_test(NAME frosted_fill_tests COMMAND frosted_fill_tests)

if(WIN32)
  set_tests_properties(
    frosted_fill_tests
    PROPERTIES
      ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt${QT_VERSION_MAJOR}::Core>")
endif()
