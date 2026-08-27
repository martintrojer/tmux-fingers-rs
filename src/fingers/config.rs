use std::collections::BTreeMap;
use std::fs::File;

use serde::{Deserialize, Serialize};

use crate::fingers::dirs;

pub const ANSI_RESET: &str = "\u{1b}[0m";

// Upstream computes the default styles by running them through the tmux style
// printer (`Tmux.style_printer.print("fg=green,bold")` and friends), which
// since upstream 2.7.0 emits 256-colour SGR sequences rather than `tput`
// output. These constants must stay byte-identical to what
// `TmuxStylePrinter::print` produces for the same style strings; the
// `default_styles_match_the_style_printer` test enforces that.
const ANSI_GREEN_BOLD: &str = "\u{1b}[38;5;2m\u{1b}[1m";
const ANSI_BLUE_BOLD: &str = "\u{1b}[38;5;4m\u{1b}[1m";
const ANSI_YELLOW: &str = "\u{1b}[38;5;3m";
const ANSI_BLUE: &str = "\u{1b}[38;5;4m";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub key: String,
    pub jump_key: String,
    pub keyboard_layout: String,
    pub patterns: BTreeMap<String, String>,
    pub alphabet: Vec<String>,
    pub benchmark_mode: String,
    pub main_action: String,
    pub ctrl_action: String,
    pub alt_action: String,
    pub shift_action: String,
    pub use_system_clipboard: bool,
    pub hint_position: String,
    pub hint_style: String,
    pub selected_hint_style: String,
    pub highlight_style: String,
    pub selected_highlight_style: String,
    pub backdrop_style: String,
    pub tmux_version: String,
    pub show_copied_notification: String,
    pub enabled_builtin_patterns: String,
    pub enable_bindings: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            key: "F".into(),
            jump_key: "J".into(),
            keyboard_layout: "qwerty".into(),
            patterns: BTreeMap::new(),
            alphabet: Vec::new(),
            benchmark_mode: "0".into(),
            main_action: ":copy:".into(),
            ctrl_action: ":open:".into(),
            alt_action: String::new(),
            shift_action: ":paste:".into(),
            use_system_clipboard: true,
            hint_position: "left".into(),
            hint_style: ANSI_GREEN_BOLD.into(),
            selected_hint_style: ANSI_BLUE_BOLD.into(),
            highlight_style: ANSI_YELLOW.into(),
            selected_highlight_style: ANSI_BLUE.into(),
            backdrop_style: String::new(),
            tmux_version: "3.1".into(),
            show_copied_notification: "0".into(),
            enabled_builtin_patterns: "all".into(),
            enable_bindings: true,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self, std::io::Error> {
        let file = File::open(dirs::config_path())?;
        serde_json::from_reader(file).map_err(std::io::Error::other)
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        dirs::ensure_folders()?;
        let file = File::create(dirs::config_path())?;
        serde_json::to_writer(file, self).map_err(std::io::Error::other)
    }

    pub fn members() -> &'static [&'static str] {
        &[
            "key",
            "jump_key",
            "keyboard_layout",
            "patterns",
            "alphabet",
            "benchmark_mode",
            "main_action",
            "ctrl_action",
            "alt_action",
            "shift_action",
            "use_system_clipboard",
            "hint_position",
            "hint_style",
            "selected_hint_style",
            "highlight_style",
            "selected_highlight_style",
            "backdrop_style",
            "tmux_version",
            "show_copied_notification",
            "enabled_builtin_patterns",
            "enable_bindings",
        ]
    }

    pub fn reset_sequence() -> &'static str {
        ANSI_RESET
    }
}

pub fn builtin_patterns() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("ip", r"\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}"),
        (
            "uuid",
            r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}",
        ),
        ("sha", r"[0-9a-f]{7,128}"),
        ("digit", r"[0-9]{4,}"),
        (
            "url",
            r#"((https?://|git@|git://|ssh://|ftp://|file:///)[^\s()"']+)"#,
        ),
        ("path", r"(([.\w\-~\$@]+)?(/[.\w\-@]+)+/?)"),
        ("hex", r"(0x[0-9a-fA-F]+)"),
        (
            "kubernetes",
            r"(deployment.app|binding|componentstatuse|configmap|endpoint|event|limitrange|namespace|node|persistentvolumeclaim|persistentvolume|pod|podtemplate|replicationcontroller|resourcequota|secret|serviceaccount|service|mutatingwebhookconfiguration.admissionregistration.k8s.io|validatingwebhookconfiguration.admissionregistration.k8s.io|customresourcedefinition.apiextension.k8s.io|apiservice.apiregistration.k8s.io|controllerrevision.apps|daemonset.apps|deployment.apps|replicaset.apps|statefulset.apps|tokenreview.authentication.k8s.io|localsubjectaccessreview.authorization.k8s.io|selfsubjectaccessreviews.authorization.k8s.io|selfsubjectrulesreview.authorization.k8s.io|subjectaccessreview.authorization.k8s.io|horizontalpodautoscaler.autoscaling|cronjob.batch|job.batch|certificatesigningrequest.certificates.k8s.io|events.events.k8s.io|daemonset.extensions|deployment.extensions|ingress.extensions|networkpolicies.extensions|podsecuritypolicies.extensions|replicaset.extensions|networkpolicie.networking.k8s.io|poddisruptionbudget.policy|clusterrolebinding.rbac.authorization.k8s.io|clusterrole.rbac.authorization.k8s.io|rolebinding.rbac.authorization.k8s.io|role.rbac.authorization.k8s.io|storageclasse.storage.k8s.io)[[:alnum:]_#$%&+=/@-]+",
        ),
        // Matches deployment-managed pod names like "nginx-deployment-66b6c48dd5-7xb2r".
        // K8s generates hashes using a restricted alphabet (no vowels, no 0/1/3)
        // to avoid accidental profanity: "bcdfghjklmnpqrstvwxz2456789"
        // See: https://github.com/kubernetes/apimachinery/blob/master/pkg/util/rand/rand.go
        (
            "kubernetes-pod",
            r"[a-z][a-z0-9-]*[a-z0-9]-[bcdfghjklmnpqrstvwxz2456789]{5,10}-[bcdfghjklmnpqrstvwxz2456789]{5}",
        ),
        (
            "git-status",
            r"(modified|deleted|deleted by us|new file): +(?<match>.+)",
        ),
        (
            "git-status-branch",
            r"Your branch is up to date with '(?<match>.*)'.",
        ),
        ("diff", r"(---|\+\+\+) [ab]/(?<match>.*)"),
    ])
}

pub fn alphabet_map() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("qwerty", "asdfqwerzxcvjklmiuopghtybn"),
        ("qwerty-homerow", "asdfjklgh"),
        ("qwerty-left-hand", "asdfqwerzcxv"),
        ("qwerty-right-hand", "jkluiopmyhn"),
        ("azerty", "qsdfazerwxcvjklmuiopghtybn"),
        ("azerty-homerow", "qsdfjkmgh"),
        ("azerty-left-hand", "qsdfazerwxcv"),
        ("azerty-right-hand", "jklmuiophyn"),
        ("qwertz", "asdfqweryxcvjkluiopmghtzbn"),
        ("qwertz-homerow", "asdfghjkl"),
        ("qwertz-left-hand", "asdfqweryxcv"),
        ("qwertz-right-hand", "jkluiopmhzn"),
        ("dvorak", "aoeuqjkxpyhtnsgcrlmwvzfidb"),
        ("dvorak-homerow", "aoeuhtnsid"),
        ("dvorak-left-hand", "aoeupqjkyix"),
        ("dvorak-right-hand", "htnsgcrlmwvz"),
        ("colemak", "arstqwfpzxcvneioluymdhgjbk"),
        ("colemak-homerow", "arstneiodh"),
        ("colemak-left-hand", "arstqwfpzxcv"),
        ("colemak-right-hand", "neioluymjhk"),
    ])
}

#[cfg(test)]
mod tests {
    use super::{Config, builtin_patterns};
    use crate::tmux_style_printer::TmuxStylePrinter;
    use pcre2::bytes::Regex;

    #[test]
    fn default_styles_match_the_style_printer() {
        let config = Config::default();
        let mut printer = TmuxStylePrinter::new();
        let mut print = |style: &str| printer.print(style, false).expect("style output");

        assert_eq!(config.hint_style, print("fg=green,bold"));
        assert_eq!(config.highlight_style, print("fg=yellow"));
        assert_eq!(config.selected_hint_style, print("fg=blue,bold"));
        assert_eq!(config.selected_highlight_style, print("fg=blue"));
        assert_eq!(config.backdrop_style, "");
    }

    fn matches_for(pattern_name: &str, input: &str) -> Vec<String> {
        let pattern = builtin_patterns()[pattern_name];
        let regex = Regex::new(pattern).expect("valid pattern");
        regex
            .find_iter(input.as_bytes())
            .map(|m| String::from_utf8(m.expect("match").as_bytes().to_vec()).unwrap())
            .collect()
    }

    #[test]
    fn kubernetes_pod_matches_deployment_managed_pod_names() {
        let input = "
          nginx-deployment-66b6c48dd5-7xb2r
          coredns-787d4945fb-9pkxr
          my-app-7d9fdc8v4h-4x2qz
        ";

        assert_eq!(
            matches_for("kubernetes-pod", input),
            vec![
                "nginx-deployment-66b6c48dd5-7xb2r",
                "coredns-787d4945fb-9pkxr",
                "my-app-7d9fdc8v4h-4x2qz",
            ]
        );
    }

    #[test]
    fn kubernetes_pod_ignores_non_pod_text() {
        let input = "
          nginx-deployment
          nginx-deployment-66b6c48dd5
          deployment.apps/nginx-deployment
          d6f4b4ac-4b78-4d79-96a1-eb9ab72f2c59
          this-is-a-normal-looking-line
        ";

        assert!(matches_for("kubernetes-pod", input).is_empty());
    }
}
