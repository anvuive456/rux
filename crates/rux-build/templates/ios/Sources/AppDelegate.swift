import RuxRuntime
import UIKit

@main
final class AppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?
    private var host: RuxHost?

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
    ) -> Bool {
        let controller = UIViewController()
        controller.view.backgroundColor = .systemBackground

        let window = UIWindow(frame: UIScreen.main.bounds)
        window.rootViewController = controller
        window.makeKeyAndVisible()
        self.window = window

        let host = RuxHost(container: controller.view)
        host.start()
        self.host = host
        return true
    }

    func applicationWillTerminate(_ application: UIApplication) {
        host?.stop()
    }
}
