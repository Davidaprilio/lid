import sys

args = sys.argv[1:]
if "--native-host" in args:
    # Started by a browser: stay light, never load Qt.
    from .native_host import main
elif "--install-browser-integration" in args or "--uninstall-browser-integration" in args:
    from .browser import main as browser_main

    def main():
        return browser_main(args)
else:
    from .app import main

sys.exit(main())
