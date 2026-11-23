# macOS AUv2/AUv3 wrappers for Nebula De-Esser.
#
# This project does not build the plugin itself. It assumes a finished CLAP
# bundle was produced by `cargo xtask bundle` and uses the vendored
# clap-wrapper to project that CLAP as an Audio Unit.
#
# AUv2 only needs the command line tools. AUv3 additionally needs a full
# Xcode installation because the .appex must be linked as an app-extension
# product and signed by Xcode's build system. Pass -G Xcode and
# -DNEBULA_BUILD_AUV3=ON to enable it.

cmake_minimum_required(VERSION 3.21)
project(nebula_macos_au LANGUAGES C CXX OBJC OBJCXX)

if(NOT APPLE)
    message(FATAL_ERROR "Nebula AU wrappers can only be built on macOS")
endif()

set(NEBULA_VERSION "3.4.0" CACHE STRING "Version written into the AU bundles")
set(NEBULA_CLAP_BUNDLE "" CACHE PATH "Path to the built Nebula De-Esser.clap bundle")
set(NEBULA_BUILD_AUV3 OFF CACHE BOOL "Also build the AUv3 appex and its host app (requires -G Xcode)")

if(NOT NEBULA_CLAP_BUNDLE)
    message(FATAL_ERROR "NEBULA_CLAP_BUNDLE must point at the built Nebula De-Esser.clap bundle")
endif()
if(NOT EXISTS "${NEBULA_CLAP_BUNDLE}/Contents/MacOS/Nebula De-Esser")
    message(FATAL_ERROR "No Nebula De-Esser.clap bundle found at ${NEBULA_CLAP_BUNDLE}")
endif()

set(CLAP_SDK_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../vendor/clap-sdk" CACHE PATH "CLAP SDK")
set(AUDIOUNIT_SDK_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../vendor/AudioUnitSDK" CACHE PATH "Apple AudioUnitSDK")
set(CLAP_WRAPPER_CMAKE_CURRENT_SOURCE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../vendor/clap-wrapper" CACHE PATH "clap-wrapper source")

if(NOT EXISTS "${CLAP_SDK_ROOT}/include/clap/clap.h")
    message(FATAL_ERROR "CLAP SDK not found at ${CLAP_SDK_ROOT}")
endif()
if(NOT EXISTS "${AUDIOUNIT_SDK_ROOT}/src/AudioUnitSDK")
    message(FATAL_ERROR "AudioUnitSDK not found at ${AUDIOUNIT_SDK_ROOT}")
endif()
if(NOT EXISTS "${CLAP_WRAPPER_CMAKE_CURRENT_SOURCE_DIR}/CMakeLists.txt")
    message(FATAL_ERROR "clap-wrapper not found at ${CLAP_WRAPPER_CMAKE_CURRENT_SOURCE_DIR}")
endif()

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

set(CLAP_WRAPPER_OUTPUT_NAME "Nebula De-Esser" CACHE STRING "")
set(CLAP_WRAPPER_BUNDLE_VERSION "${NEBULA_VERSION}" CACHE STRING "")
set(CLAP_WRAPPER_COPY_AFTER_BUILD OFF CACHE BOOL "")
set(CLAP_WRAPPER_DOWNLOAD_DEPENDENCIES OFF CACHE BOOL "")

# Keep the products together. Multi-config generators (Xcode) append the
# configuration name below this directory.
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/artifacts")
set(CMAKE_LIBRARY_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/artifacts")
set(CMAKE_OSX_DEPLOYMENT_TARGET "11.0" CACHE STRING "Minimum macOS version")

# clap-wrapper sets up its SDK targets in this subdirectory. Its own default
# formats are skipped because it is not the top level project; only the
# targets created below are built.
add_subdirectory("${CLAP_WRAPPER_CMAKE_CURRENT_SOURCE_DIR}" clap-wrapper)

# ---------------------------------------------------------------------------
# AUv2 (.component)
# ---------------------------------------------------------------------------
add_library(nebula_auv2 MODULE)
target_add_auv2_wrapper(
    TARGET nebula_auv2
    OUTPUT_NAME "Nebula De-Esser"
    BUNDLE_IDENTIFIER "audio.nebula.deesser.auv2"
    BUNDLE_VERSION "${NEBULA_VERSION}"
    INSTRUMENT_TYPE "aufx"
    MANUFACTURER_NAME "Nebula Audio"
    MANUFACTURER_CODE "NbAu"
    SUBTYPE_CODE "NDeS"
    MACOS_EMBEDDED_CLAP_LOCATION "${NEBULA_CLAP_BUNDLE}"
)

# Ad-hoc sign the embedded CLAP and the component so the result is internally
# consistent. This is not notarization; it simply lets macOS validate nested
# code and keeps AUv3 appex signatures intact.
add_custom_command(
    TARGET nebula_auv2 POST_BUILD
    COMMAND codesign --force --sign - --timestamp=none
            "$<TARGET_BUNDLE_DIR:nebula_auv2>/Contents/PlugIns/Nebula De-Esser.clap"
    COMMAND codesign --force --sign - --timestamp=none
            "$<TARGET_BUNDLE_DIR:nebula_auv2>"
    COMMENT "Ad-hoc signing the AUv2 component and embedded CLAP"
)

# ---------------------------------------------------------------------------
# AUv3 (.appex + the host .app that registers it)
# ---------------------------------------------------------------------------
if(NEBULA_BUILD_AUV3)
    if(NOT CMAKE_GENERATOR STREQUAL "Xcode")
        message(FATAL_ERROR "AUv3 requires the Xcode generator: configure with -G Xcode")
    endif()

    set(NEBULA_AUV3_ENTITLEMENTS
        "${CLAP_WRAPPER_CMAKE_CURRENT_SOURCE_DIR}/src/detail/auv3/auv3.entitlements")

    add_executable(nebula_auv3)
    target_add_auv3_wrapper(
        TARGET nebula_auv3
        OUTPUT_NAME "Nebula De-Esser"
        # App extension identifiers must be prefixed by the host app's
        # identifier for PlugInKit to register them.
        BUNDLE_IDENTIFIER "audio.nebula.deesser.auv3standalone.auv3"
        BUNDLE_VERSION "${NEBULA_VERSION}"
        INSTRUMENT_TYPE "aufx"
        MANUFACTURER_NAME "Nebula Audio"
        MANUFACTURER_CODE "NbAu"
        SUBTYPE_CODE "NDeS"
        ENTITLEMENTS "${NEBULA_AUV3_ENTITLEMENTS}"
        MACOS_EMBEDDED_CLAP_LOCATION "${NEBULA_CLAP_BUNDLE}"
    )

    add_executable(nebula_auv3_host)
    target_add_auv3_standalone_wrapper(
        TARGET nebula_auv3_host
        OUTPUT_NAME "Nebula De-Esser AUv3"
        BUNDLE_IDENTIFIER "audio.nebula.deesser.auv3standalone"
        BUNDLE_VERSION "${NEBULA_VERSION}"
        AUV3_TARGET nebula_auv3
        AU_TYPE "aufx"
        AU_SUBTYPE "NDeS"
        AU_MANUFACTURER "NbAu"
    )

    # Xcode creates the appex bundle in the product directory, while
    # clap-wrapper's PRE_BUILD CLAP copy is relative to LIBRARY_OUTPUT_DIRECTORY
    # (unset for executable bundles). Embed the CLAP into the appex copy that
    # actually ships inside the host app, then sign from the inside out:
    # CLAP -> appex (with sandbox entitlements) -> host app.
    #
    # This must happen on the host target, after target_embed_auv3_appex has
    # copied the appex into Contents/PlugIns; the vendor pipeline trusts that
    # copy to keep a valid entitlement-bearing appex signature.
    set(NEBULA_AUV3_APPEX_PATH
        "$<TARGET_BUNDLE_DIR:nebula_auv3_host>/Contents/PlugIns/Nebula De-Esser.appex")
    add_custom_command(
        TARGET nebula_auv3_host POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E make_directory
                "${NEBULA_AUV3_APPEX_PATH}/Contents/PlugIns"
        COMMAND ${CMAKE_COMMAND} -E rm -rf
                "${NEBULA_AUV3_APPEX_PATH}/Contents/PlugIns/Nebula De-Esser.clap"
        COMMAND ${CMAKE_COMMAND} -E copy_directory
                "${NEBULA_CLAP_BUNDLE}"
                "${NEBULA_AUV3_APPEX_PATH}/Contents/PlugIns/Nebula De-Esser.clap"
        COMMAND codesign --force --sign - --timestamp=none
                "${NEBULA_AUV3_APPEX_PATH}/Contents/PlugIns/Nebula De-Esser.clap"
        COMMAND codesign --force --sign - --timestamp=none
                --entitlements "${NEBULA_AUV3_ENTITLEMENTS}"
                "${NEBULA_AUV3_APPEX_PATH}"
        COMMAND codesign --force --sign - --timestamp=none
                "$<TARGET_BUNDLE_DIR:nebula_auv3_host>"
        COMMENT "Embedding and ad-hoc signing the AUv3 appex"
    )
endif()
