# Nebula De-Esser — AUv2 wrapper for macOS
#
# Builds the Audio Unit v2 component that Steinberg ships with the VST 3 SDK.
# The component is a thin loadable bundle: it contains a small AU wrapper
# executable and embeds the real plugin as `Contents/Resources/plugin.vst3`.
# Hosts load the VST3 through the wrapper, so there is exactly one DSP/UI
# implementation to maintain.
#
# Reference:
#   https://steinbergmedia.github.io/vst3_dev_portal/pages/What+is+the+VST+3+SDK/Wrappers/AUv2+Wrapper.html
#   vendor/vst3sdk/public.sdk/source/vst/auwrapper/
#
# Usage:
#   cmake -S cmake/macos-au -B build-au -G Ninja \
#       -DCMAKE_BUILD_TYPE=Release \
#       -DCMAKE_OSX_ARCHITECTURES=arm64 \
#       -DNEBULA_VST3_PATH=/path/to/Nebula\ De-Esser.vst3
#   cmake --build build-au
#
# The finished component is written to:
#   <build-dir>/bundled/Nebula De-Esser.component

cmake_minimum_required(VERSION 3.20)
project(NebulaDeEsserAU LANGUAGES CXX OBJCXX)

if(NOT APPLE)
    message(FATAL_ERROR "The AUv2 wrapper can only be built on macOS.")
endif()

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_OBJCXX_STANDARD 17)
set(CMAKE_OBJCXX_STANDARD_REQUIRED ON)

if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
    set(CMAKE_BUILD_TYPE Release CACHE STRING "Build type" FORCE)
endif()

set(CMAKE_OSX_DEPLOYMENT_TARGET "10.13" CACHE STRING "Minimum macOS version")

set(PLUGIN_DISPLAY_NAME "Nebula De-Esser" CACHE STRING "Plugin display name")
set(PLUGIN_BUNDLE_ID "com.nebulaaudio.nebula-deesser.au" CACHE STRING "AU bundle identifier")
set(PLUGIN_MANUFACTURER "NbAu" CACHE STRING "AU manufacturer code")
set(PLUGIN_SUBTYPE "NDeS" CACHE STRING "AU subtype code")
set(PLUGIN_TYPE "aufx" CACHE STRING "AU component type")
set(PLUGIN_VERSION_STRING "3.4.0" CACHE STRING "Plugin version string")
set(PLUGIN_VERSION_INT "197632" CACHE STRING "AU version as decimal (0x030400 = 3.4.0)")

set(NEBULA_VST3_PATH "" CACHE PATH "Path to the built Nebula De-Esser.vst3 bundle")
if(NOT NEBULA_VST3_PATH)
    message(FATAL_ERROR "Set -DNEBULA_VST3_PATH=/path/to/Nebula De-Esser.vst3")
endif()
if(NOT EXISTS "${NEBULA_VST3_PATH}")
    message(FATAL_ERROR "VST3 bundle not found: ${NEBULA_VST3_PATH}")
endif()

get_filename_component(NEBULA_REPO_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../.." ABSOLUTE)
set(VST3SDK_ROOT "${NEBULA_REPO_ROOT}/vendor/vst3sdk" CACHE PATH "Vendored VST 3 SDK root")
set(AUDIOUNITSDK_ROOT "${NEBULA_REPO_ROOT}/vendor/AudioUnitSDK" CACHE PATH "Vendored AudioUnitSDK root")

if(NOT EXISTS "${VST3SDK_ROOT}/public.sdk/source/vst/auwrapper/auwrapper.mm")
    message(FATAL_ERROR "VST 3 SDK sources missing under ${VST3SDK_ROOT}")
endif()
if(NOT EXISTS "${AUDIOUNITSDK_ROOT}/include/AudioUnitSDK/AUBase.h")
    message(FATAL_ERROR "AudioUnitSDK sources missing under ${AUDIOUNITSDK_ROOT}")
endif()

# ---------------------------------------------------------------------------
# Shared SDK helper libraries
# ---------------------------------------------------------------------------

add_library(vst3_pluginterfaces STATIC
    "${VST3SDK_ROOT}/pluginterfaces/base/conststringtable.cpp"
    "${VST3SDK_ROOT}/pluginterfaces/base/coreiids.cpp"
    "${VST3SDK_ROOT}/pluginterfaces/base/funknown.cpp"
    "${VST3SDK_ROOT}/pluginterfaces/base/ustring.cpp"
)
target_include_directories(vst3_pluginterfaces PUBLIC "${VST3SDK_ROOT}")

add_library(vst3_base STATIC
    "${VST3SDK_ROOT}/base/source/baseiids.cpp"
    "${VST3SDK_ROOT}/base/source/fbuffer.cpp"
    "${VST3SDK_ROOT}/base/source/fdebug.cpp"
    "${VST3SDK_ROOT}/base/source/fdynlib.cpp"
    "${VST3SDK_ROOT}/base/source/fobject.cpp"
    "${VST3SDK_ROOT}/base/source/fstreamer.cpp"
    "${VST3SDK_ROOT}/base/source/fstring.cpp"
    "${VST3SDK_ROOT}/base/source/timer.cpp"
    "${VST3SDK_ROOT}/base/source/updatehandler.cpp"
    "${VST3SDK_ROOT}/base/thread/source/fcondition.cpp"
    "${VST3SDK_ROOT}/base/thread/source/flock.cpp"
)
target_link_libraries(vst3_base PUBLIC vst3_pluginterfaces)
target_include_directories(vst3_base PUBLIC "${VST3SDK_ROOT}")
target_compile_definitions(vst3_base PUBLIC RELEASE=1)

add_library(vst3_common STATIC
    "${VST3SDK_ROOT}/public.sdk/source/common/commoniids.cpp"
    "${VST3SDK_ROOT}/public.sdk/source/common/commonstringconvert.cpp"
    "${VST3SDK_ROOT}/public.sdk/source/common/openurl.cpp"
    "${VST3SDK_ROOT}/public.sdk/source/common/readfile.cpp"
    "${VST3SDK_ROOT}/public.sdk/source/common/systemclipboard_mac.mm"
    "${VST3SDK_ROOT}/public.sdk/source/common/threadchecker_mac.mm"
    "${VST3SDK_ROOT}/public.sdk/source/vst/vstpresetfile.cpp"
)
target_link_libraries(vst3_common PUBLIC vst3_base)
target_include_directories(vst3_common PUBLIC "${VST3SDK_ROOT}")

add_library(vst3_hosting STATIC
    "${VST3SDK_ROOT}/public.sdk/source/vst/hosting/connectionproxy.cpp"
    "${VST3SDK_ROOT}/public.sdk/source/vst/hosting/eventlist.cpp"
    "${VST3SDK_ROOT}/public.sdk/source/vst/hosting/hostclasses.cpp"
    "${VST3SDK_ROOT}/public.sdk/source/vst/hosting/module.cpp"
    "${VST3SDK_ROOT}/public.sdk/source/vst/hosting/parameterchanges.cpp"
    "${VST3SDK_ROOT}/public.sdk/source/vst/hosting/pluginterfacesupport.cpp"
    "${VST3SDK_ROOT}/public.sdk/source/vst/hosting/processdata.cpp"
    "${VST3SDK_ROOT}/public.sdk/source/vst/utility/stringconvert.cpp"
    "${VST3SDK_ROOT}/public.sdk/source/vst/vstinitiids.cpp"
)
target_link_libraries(vst3_hosting PUBLIC vst3_common)
target_include_directories(vst3_hosting PUBLIC "${VST3SDK_ROOT}")

# Apple's AudioUnitSDK provides the AUBase/AUMIDIBase implementation the
# wrapper subclasses. It is built directly from source so no Xcode project or
# extra SDK installation is required.
add_library(audiounit_sdk STATIC
    "${AUDIOUNITSDK_ROOT}/src/AudioUnitSDK/AUBase.cpp"
    "${AUDIOUNITSDK_ROOT}/src/AudioUnitSDK/AUBuffer.cpp"
    "${AUDIOUNITSDK_ROOT}/src/AudioUnitSDK/AUBufferAllocator.cpp"
    "${AUDIOUNITSDK_ROOT}/src/AudioUnitSDK/AUEffectBase.cpp"
    "${AUDIOUNITSDK_ROOT}/src/AudioUnitSDK/AUInputElement.cpp"
    "${AUDIOUNITSDK_ROOT}/src/AudioUnitSDK/AUMIDIBase.cpp"
    "${AUDIOUNITSDK_ROOT}/src/AudioUnitSDK/AUMIDIEffectBase.cpp"
    "${AUDIOUNITSDK_ROOT}/src/AudioUnitSDK/AUOutputElement.cpp"
    "${AUDIOUNITSDK_ROOT}/src/AudioUnitSDK/AUPlugInDispatch.cpp"
    "${AUDIOUNITSDK_ROOT}/src/AudioUnitSDK/AUScopeElement.cpp"
    "${AUDIOUNITSDK_ROOT}/src/AudioUnitSDK/ComponentBase.cpp"
    "${AUDIOUNITSDK_ROOT}/src/AudioUnitSDK/MusicDeviceBase.cpp"
)
target_include_directories(audiounit_sdk PUBLIC "${AUDIOUNITSDK_ROOT}/include")
target_link_libraries(audiounit_sdk PUBLIC
    "-framework AudioToolbox"
    "-framework AudioUnit"
    "-framework CoreAudio"
    "-framework CoreFoundation"
    "-framework CoreMIDI"
)

# ---------------------------------------------------------------------------
# AUv2 wrapper bundle
# ---------------------------------------------------------------------------

set(AU_WRAPPER_SOURCES
    "${VST3SDK_ROOT}/public.sdk/source/vst/auwrapper/auwrapper.mm"
    "${VST3SDK_ROOT}/public.sdk/source/vst/auwrapper/auwrapper.h"
    "${VST3SDK_ROOT}/public.sdk/source/vst/auwrapper/aucocoaview.mm"
    "${VST3SDK_ROOT}/public.sdk/source/vst/auwrapper/aucocoaview.h"
    "${VST3SDK_ROOT}/public.sdk/source/vst/auwrapper/NSDataIBStream.mm"
    "${VST3SDK_ROOT}/public.sdk/source/vst/auwrapper/NSDataIBStream.h"
)

add_library(NebulaDeEsserAU MODULE ${AU_WRAPPER_SOURCES})

target_compile_definitions(NebulaDeEsserAU PRIVATE
    SMTG_AUWRAPPER_USES_AUSDK=1
    CA_USE_AUDIO_PLUGIN_ONLY=0
    SMTG_AUCocoaUIBase_CLASS_NAME=SMTG_AUCocoaUIBase_NebulaDeEsser
)

target_compile_options(NebulaDeEsserAU PRIVATE
    -Wno-deprecated-declarations
    -Wno-unguarded-availability-new
)

target_link_libraries(NebulaDeEsserAU PRIVATE
    vst3_hosting
    audiounit_sdk
    "-framework AudioUnit"
    "-framework AudioToolbox"
    "-framework CoreAudio"
    "-framework CoreFoundation"
    "-framework CoreMIDI"
    "-framework Carbon"
    "-framework Cocoa"
)

# A component is a plain loadable bundle; CMake's bundle handling differs
# between generators, so the .component directory is assembled explicitly.
set_target_properties(NebulaDeEsserAU PROPERTIES
    PREFIX ""
    SUFFIX ""
    OUTPUT_NAME "${PLUGIN_DISPLAY_NAME}"
)

configure_file(
    "${CMAKE_CURRENT_SOURCE_DIR}/Info.plist.in"
    "${CMAKE_CURRENT_BINARY_DIR}/Info.plist"
    @ONLY
)

set(AU_BUNDLE_DIR "${CMAKE_BINARY_DIR}/bundled/${PLUGIN_DISPLAY_NAME}.component")

# A custom target (rather than a POST_BUILD step) so the component is
# reassembled on every build. This guarantees that a rebuilt VST3 is embedded
# even when the AU wrapper sources themselves did not change.
add_custom_target(NebulaDeEsserAUBundle ALL
    COMMAND "${CMAKE_COMMAND}" -E rm -rf "${AU_BUNDLE_DIR}"
    COMMAND "${CMAKE_COMMAND}" -E make_directory "${AU_BUNDLE_DIR}/Contents/MacOS"
    COMMAND "${CMAKE_COMMAND}" -E make_directory "${AU_BUNDLE_DIR}/Contents/Resources"
    COMMAND "${CMAKE_COMMAND}" -E copy
            "$<TARGET_FILE:NebulaDeEsserAU>"
            "${AU_BUNDLE_DIR}/Contents/MacOS/${PLUGIN_DISPLAY_NAME}"
    COMMAND "${CMAKE_COMMAND}" -E copy
            "${CMAKE_CURRENT_BINARY_DIR}/Info.plist"
            "${AU_BUNDLE_DIR}/Contents/Info.plist"
    COMMAND "${CMAKE_COMMAND}" -E copy_directory
            "${NEBULA_VST3_PATH}"
            "${AU_BUNDLE_DIR}/Contents/Resources/plugin.vst3"
    COMMAND codesign --force --sign - "${AU_BUNDLE_DIR}/Contents/Resources/plugin.vst3"
    COMMAND codesign --force --sign - "${AU_BUNDLE_DIR}"
    DEPENDS NebulaDeEsserAU
    COMMENT "Assembling ${PLUGIN_DISPLAY_NAME}.component"
    VERBATIM
)

message(STATUS "AUv2 component output: ${AU_BUNDLE_DIR}")
message(STATUS "Embedded VST3: ${NEBULA_VST3_PATH}")
