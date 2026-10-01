#include <iostream>
#include <string>
#include <vector>
#include <windows.h>

namespace {

// Quotes one argument the way CommandLineToArgvW reads it back
std::wstring quoteArg(const std::wstring& arg)
{
    if (!arg.empty() && arg.find_first_of(L" \t\"") == std::wstring::npos) {
        return arg;
    }
    std::wstring quoted = L"\"";
    size_t backslashes = 0;
    for (const wchar_t c : arg) {
        if (c == L'\\') {
            ++backslashes;
            continue;
        }
        if (c == L'"') {
            quoted.append(backslashes * 2 + 1, L'\\');
        } else {
            quoted.append(backslashes, L'\\');
        }
        backslashes = 0;
        quoted += c;
    }
    quoted.append(backslashes * 2, L'\\');
    quoted += L'"';
    return quoted;
}

std::wstring guiExecutablePath()
{
    // Grown until it fits: an MSIX install lives under a WindowsApps path
    // far longer than MAX_PATH allows for in older code
    std::vector<wchar_t> buffer(MAX_PATH);
    for (;;) {
        const DWORD length = GetModuleFileNameW(
          nullptr, buffer.data(), static_cast<DWORD>(buffer.size()));
        if (length == 0) {
            return L"phramer.exe";
        }
        if (length < buffer.size()) {
            std::wstring path(buffer.data(), length);
            const size_t slash = path.find_last_of(L'\\');
            const std::wstring directory =
              slash != std::wstring::npos ? path.substr(0, slash + 1) : L"";
            return directory + L"phramer.exe";
        }
        buffer.resize(buffer.size() * 2);
    }
}

// Runs phramer.exe directly rather than through a shell: cmd.exe is not
// needed, and an MSIX package should not start it. Returns 1 only when it
// could not be started; the exit code was never relayed and scripts may
// rely on that.
int callPhramer(int argc, wchar_t* argv[], bool wait)
{
    std::wstring commandLine = quoteArg(guiExecutablePath());
    for (int i = 1; i < argc; ++i) {
        commandLine += L' ';
        commandLine += quoteArg(argv[i]);
    }

    SECURITY_ATTRIBUTES inherit{};
    inherit.nLength = sizeof(inherit);
    inherit.bInheritHandle = TRUE;

    HANDLE readEnd = nullptr;
    HANDLE writeEnd = nullptr;
    if (wait && CreatePipe(&readEnd, &writeEnd, &inherit, 0)) {
        // Only the child's copy of the write end may be inherited
        SetHandleInformation(readEnd, HANDLE_FLAG_INHERIT, 0);
    }

    STARTUPINFOW startup{};
    startup.cb = sizeof(startup);
    if (writeEnd) {
        startup.dwFlags = STARTF_USESTDHANDLES;
        startup.hStdInput = GetStdHandle(STD_INPUT_HANDLE);
        startup.hStdOutput = writeEnd;
        // As before: only stdout is relayed, so Qt's diagnostics on stderr
        // do not end up in a script's output
        startup.hStdError = GetStdHandle(STD_ERROR_HANDLE);
    }

    PROCESS_INFORMATION process{};
    // CreateProcessW may write to the command line buffer
    std::vector<wchar_t> mutableLine(commandLine.begin(), commandLine.end());
    mutableLine.push_back(L'\0');
    const BOOL started = CreateProcessW(nullptr,
                                        mutableLine.data(),
                                        nullptr,
                                        nullptr,
                                        writeEnd ? TRUE : FALSE,
                                        0,
                                        nullptr,
                                        nullptr,
                                        &startup,
                                        &process);
    // Closed here so the pipe reports end-of-file once the child exits
    if (writeEnd) {
        CloseHandle(writeEnd);
    }
    if (!started) {
        if (readEnd) {
            CloseHandle(readEnd);
        }
        std::cerr << "Could not start phramer.exe (error " << GetLastError()
                  << ")" << std::endl;
        return 1;
    }

    if (wait) {
        if (readEnd) {
            char buffer[2048];
            DWORD read = 0;
            while (ReadFile(readEnd, buffer, sizeof(buffer), &read, nullptr) &&
                   read > 0) {
                std::cout.write(buffer, read);
            }
            CloseHandle(readEnd);
        }
        WaitForSingleObject(process.hProcess, INFINITE);
    }
    CloseHandle(process.hThread);
    CloseHandle(process.hProcess);
    return 0;
}

} // namespace

// Console 'wrapper' for phramer on windows
int wmain(int argc, wchar_t* argv[])
{
    int exitCode = 0;
    // if no args, do not wait for stdout
    if (argc == 1) {
        std::cout << "Starting phramer in daemon mode" << std::endl;
        exitCode = callPhramer(argc, argv, false);
    } else {
        exitCode = callPhramer(argc, argv, true);
    }
    std::cout.flush();
    return exitCode;
}
