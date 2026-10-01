// SPDX-License-Identifier: GPL-3.0-or-later

#include "utils/packagedstartup.h"
#include "utils/packageidentity.h"

#include <QDesktopServices>
#include <QUrl>

#include <winrt/Windows.ApplicationModel.h>
#include <winrt/Windows.Foundation.h>

#include <thread>

namespace {

constexpr wchar_t TaskId[] = L"PhramerStartup";

using winrt::Windows::ApplicationModel::StartupTaskState;

PackagedStartup::State fromWinRt(StartupTaskState state)
{
    using PackagedStartup::State;
    switch (state) {
        case StartupTaskState::Enabled:
        case StartupTaskState::EnabledByPolicy:
            return State::Enabled;
        case StartupTaskState::DisabledByUser:
            return State::DisabledByUser;
        case StartupTaskState::DisabledByPolicy:
            return State::DisabledByPolicy;
        default:
            return State::Disabled;
    }
}

// Waiting on a WinRT operation is not allowed on a single-threaded
// apartment such as the GUI thread's, so each call runs on a thread of its
// own with a multi-threaded apartment, as utils/snippingtool does. The calls
// return at once for a packaged desktop app, which gets no consent dialog.
PackagedStartup::State run(int request)
{
    using PackagedStartup::State;
    State result = State::Unavailable;
    std::thread worker([&result, request]() {
        bool initialised = false;
        try {
            winrt::init_apartment(winrt::apartment_type::multi_threaded);
            initialised = true;
            auto task =
              winrt::Windows::ApplicationModel::StartupTask::GetAsync(TaskId)
                .get();
            if (request > 0) {
                result = fromWinRt(task.RequestEnableAsync().get());
            } else {
                if (request == 0) {
                    task.Disable();
                }
                result = fromWinRt(task.State());
            }
        } catch (const winrt::hresult_error&) {
            result = State::Unavailable;
        }
        if (initialised) {
            winrt::uninit_apartment();
        }
    });
    worker.join();
    return result;
}

} // namespace

namespace PackagedStartup {

State state()
{
    if (!PackageIdentity::isPackaged()) {
        return State::Unavailable;
    }
    return run(-1);
}

State setEnabled(bool enable)
{
    if (!PackageIdentity::isPackaged()) {
        return State::Unavailable;
    }
    return run(enable ? 1 : 0);
}

void openStartupSettings()
{
    QDesktopServices::openUrl(QUrl(QStringLiteral("ms-settings:startupapps")));
}

} // namespace PackagedStartup
