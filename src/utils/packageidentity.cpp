// SPDX-License-Identifier: GPL-3.0-or-later

#include "utils/packageidentity.h"

#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>

#include <appmodel.h>
#include <shobjidl.h>
#include <wrl/client.h>

#include <vector>

using Microsoft::WRL::ComPtr;

namespace PackageIdentity {

bool isPackaged()
{
    UINT32 length = 0;
    // APPMODEL_ERROR_NO_PACKAGE is the documented answer for a process
    // without identity; with one, a null buffer reports its size instead
    return GetCurrentPackageFullName(&length, nullptr) !=
           APPMODEL_ERROR_NO_PACKAGE;
}

QString familyName()
{
    UINT32 length = 0;
    if (GetCurrentPackageFamilyName(&length, nullptr) !=
        ERROR_INSUFFICIENT_BUFFER) {
        return {};
    }
    std::vector<wchar_t> buffer(length);
    if (GetCurrentPackageFamilyName(&length, buffer.data()) != ERROR_SUCCESS) {
        return {};
    }
    return QString::fromWCharArray(buffer.data());
}

QString appUserModelId()
{
    UINT32 length = 0;
    if (GetCurrentApplicationUserModelId(&length, nullptr) !=
        ERROR_INSUFFICIENT_BUFFER) {
        return {};
    }
    std::vector<wchar_t> buffer(length);
    if (GetCurrentApplicationUserModelId(&length, buffer.data()) !=
        ERROR_SUCCESS) {
        return {};
    }
    return QString::fromWCharArray(buffer.data());
}

bool activateSelf()
{
    const QString aumid = appUserModelId();
    if (aumid.isEmpty()) {
        return false;
    }

    const HRESULT init = CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);
    const bool ownApartment = SUCCEEDED(init);

    bool activated = false;
    {
        ComPtr<IApplicationActivationManager> manager;
        if (SUCCEEDED(CoCreateInstance(CLSID_ApplicationActivationManager,
                                       nullptr,
                                       CLSCTX_LOCAL_SERVER,
                                       IID_PPV_ARGS(&manager)))) {
            DWORD processId = 0;
            activated = SUCCEEDED(manager->ActivateApplication(
              aumid.toStdWString().c_str(), nullptr, AO_NONE, &processId));
        }
    }

    if (ownApartment) {
        CoUninitialize();
    }
    return activated;
}

} // namespace PackageIdentity
