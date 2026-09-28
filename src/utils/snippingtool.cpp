// SPDX-License-Identifier: GPL-3.0-or-later

#include "snippingtool.h"

#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>

#include <appmodel.h>
#include <shobjidl.h>
#include <wrl/client.h>

#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.System.h>

#include <atomic>
#include <chrono>
#include <thread>

using Microsoft::WRL::ComPtr;

namespace {

constexpr wchar_t PackageFamily[] = L"Microsoft.ScreenSketch_8wekyb3d8bbwe";
constexpr wchar_t ApplicationId[] = L"Microsoft.ScreenSketch_8wekyb3d8bbwe!App";
// The form Windows sends for Win+Shift+R, as Snipping Tool itself spells it
constexpr wchar_t RecordingUri[] =
  L"ms-screenclip:capture?mode=default&type=recording&source=Phramer";

// Launches started by startRecordingAsync() that have not reported back,
// and whether the application has begun to quit. A pending launch's callback
// reaches into the application, so it must not run once that is going away.
std::atomic<int> pendingLaunches{ 0 };
std::atomic<bool> quitting{ false };

// Runs on a thread of its own that has no apartment yet. Waiting on a WinRT
// operation is not allowed on a single-threaded apartment such as the GUI
// thread's, so this is never called there.
bool launchRecordingLink()
{
    bool launched = false;
    bool initialised = false;
    try {
        winrt::init_apartment(winrt::apartment_type::multi_threaded);
        initialised = true;
        winrt::Windows::System::LauncherOptions options;
        options.TargetApplicationPackageFamilyName(PackageFamily);
        launched = winrt::Windows::System::Launcher::LaunchUriAsync(
                     winrt::Windows::Foundation::Uri(RecordingUri), options)
                     .get();
    } catch (const winrt::hresult_error&) {
        launched = false;
    }
    if (initialised) {
        winrt::uninit_apartment();
    }
    return launched;
}

} // namespace

namespace SnippingTool {

bool isAvailable()
{
    UINT32 count = 0;
    UINT32 bufferLength = 0;
    // A first call with no buffers only reports the sizes. Success with a
    // count of zero, or ERROR_INSUFFICIENT_BUFFER, are the two answers;
    // anything else means the package is not there for this user.
    const LONG status = FindPackagesByPackageFamily(PackageFamily,
                                                    PACKAGE_FILTER_HEAD,
                                                    &count,
                                                    nullptr,
                                                    &bufferLength,
                                                    nullptr,
                                                    nullptr);
    return (status == ERROR_SUCCESS || status == ERROR_INSUFFICIENT_BUFFER) &&
           count > 0;
}

bool launch()
{
    // The GUI thread already has an apartment from Qt; this only matters if
    // it is ever called from somewhere that does not
    const HRESULT init = CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);
    const bool ownApartment = SUCCEEDED(init);

    bool launched = false;
    {
        ComPtr<IApplicationActivationManager> manager;
        if (SUCCEEDED(CoCreateInstance(CLSID_ApplicationActivationManager,
                                       nullptr,
                                       CLSCTX_LOCAL_SERVER,
                                       IID_PPV_ARGS(&manager)))) {
            DWORD processId = 0;
            launched = SUCCEEDED(manager->ActivateApplication(
              ApplicationId, nullptr, AO_NONE, &processId));
        }
    }

    if (ownApartment) {
        CoUninitialize();
    }
    return launched;
}

bool startRecording()
{
    // Waited for: this is the short-lived process Windows starts for a link,
    // which would otherwise exit before Windows acted on the launch
    bool launched = false;
    std::thread worker(
      [&launched]() { launched = launchRecordingLink() || launch(); });
    worker.join();
    return launched;
}

void startRecordingAsync(std::function<void(bool)> done)
{
    // Not joined: the caller is the GUI thread, which must not stall on
    // Windows. The thread touches nothing of the caller's; done is all it
    // shares, and the caller marshals the answer back itself.
    ++pendingLaunches;
    std::thread([done = std::move(done)]() {
        const bool launched = launchRecordingLink() || launch();
        if (done && !quitting) {
            done(launched);
        }
        --pendingLaunches;
    }).detach();
}

void finishPendingLaunches()
{
    // Called while the application is still whole, before anything is torn
    // down. From here no callback runs; one already running gets a moment to
    // finish posting its message. A launch Windows still has not answered is
    // left to finish on its own: it no longer touches anything of ours.
    quitting = true;
    const auto deadline =
      std::chrono::steady_clock::now() + std::chrono::seconds(3);
    while (pendingLaunches > 0 && std::chrono::steady_clock::now() < deadline) {
        std::this_thread::sleep_for(std::chrono::milliseconds(20));
    }
}

} // namespace SnippingTool
