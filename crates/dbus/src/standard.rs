//! The interfaces every object has: Introspectable's XML, and the
//! machine id Peer reports.

use std::collections::BTreeSet;
use std::io;

use crate::names::ObjectPath;
use crate::server::{Access, Export, InterfaceSpec};

const DOCTYPE: &str = "<!DOCTYPE node PUBLIC \"-//freedesktop//DTD D-BUS Object Introspection 1.0//EN\"\n \"http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd\">\n";

const INTROSPECTABLE_XML: &str = "  <interface name=\"org.freedesktop.DBus.Introspectable\">
    <method name=\"Introspect\">
      <arg name=\"xml\" type=\"s\" direction=\"out\"/>
    </method>
  </interface>
";

const PEER_XML: &str = "  <interface name=\"org.freedesktop.DBus.Peer\">
    <method name=\"Ping\"/>
    <method name=\"GetMachineId\">
      <arg name=\"machine_uuid\" type=\"s\" direction=\"out\"/>
    </method>
  </interface>
";

const PROPERTIES_XML: &str = "  <interface name=\"org.freedesktop.DBus.Properties\">
    <method name=\"Get\">
      <arg name=\"interface_name\" type=\"s\" direction=\"in\"/>
      <arg name=\"property_name\" type=\"s\" direction=\"in\"/>
      <arg name=\"value\" type=\"v\" direction=\"out\"/>
    </method>
    <method name=\"GetAll\">
      <arg name=\"interface_name\" type=\"s\" direction=\"in\"/>
      <arg name=\"props\" type=\"a{sv}\" direction=\"out\"/>
    </method>
    <method name=\"Set\">
      <arg name=\"interface_name\" type=\"s\" direction=\"in\"/>
      <arg name=\"property_name\" type=\"s\" direction=\"in\"/>
      <arg name=\"value\" type=\"v\" direction=\"in\"/>
    </method>
    <signal name=\"PropertiesChanged\">
      <arg name=\"interface_name\" type=\"s\"/>
      <arg name=\"changed_properties\" type=\"a{sv}\"/>
      <arg name=\"invalidated_properties\" type=\"as\"/>
    </signal>
  </interface>
";

/// The XML for `path`: the standard interfaces, every interface exported
/// there, and a `<node>` for each child path that leads to an export.
pub(crate) fn introspect<'a>(
    path: &ObjectPath,
    exports: impl Iterator<Item = &'a Export>,
) -> String {
    let all: Vec<&Export> = exports.collect();
    let here: Vec<&Export> = all.iter().copied().filter(|e| e.path == *path).collect();
    let mut x = String::from(DOCTYPE);
    x.push_str("<node>\n");
    x.push_str(INTROSPECTABLE_XML);
    x.push_str(PEER_XML);
    if !here.is_empty() {
        x.push_str(PROPERTIES_XML);
    }
    for e in &here {
        interface(&mut x, &e.spec);
    }
    let prefix = if path.as_str() == "/" {
        "/".to_owned()
    } else {
        format!("{}/", path.as_str())
    };
    let children: BTreeSet<&str> = all
        .iter()
        .filter_map(|e| e.path.as_str().strip_prefix(prefix.as_str()))
        .filter(|rest| !rest.is_empty())
        .map(|rest| rest.split('/').next().unwrap())
        .collect();
    for c in children {
        x.push_str(&format!("  <node name=\"{c}\"/>\n"));
    }
    x.push_str("</node>\n");
    x
}

fn interface(x: &mut String, spec: &InterfaceSpec) {
    x.push_str(&format!("  <interface name=\"{}\">\n", spec.name));
    for m in &spec.methods {
        if m.input.is_empty() && m.output.is_empty() {
            x.push_str(&format!("    <method name=\"{}\"/>\n", m.name));
            continue;
        }
        x.push_str(&format!("    <method name=\"{}\">\n", m.name));
        for t in m.input.types() {
            x.push_str(&format!("      <arg type=\"{t}\" direction=\"in\"/>\n"));
        }
        for t in m.output.types() {
            x.push_str(&format!("      <arg type=\"{t}\" direction=\"out\"/>\n"));
        }
        x.push_str("    </method>\n");
    }
    for p in &spec.properties {
        let access = match p.access {
            Access::Read => "read",
            Access::ReadWrite => "readwrite",
        };
        x.push_str(&format!(
            "    <property name=\"{}\" type=\"{}\" access=\"{access}\"/>\n",
            p.name, p.sig
        ));
    }
    for s in &spec.signals {
        if s.sig.is_empty() {
            x.push_str(&format!("    <signal name=\"{}\"/>\n", s.name));
            continue;
        }
        x.push_str(&format!("    <signal name=\"{}\">\n", s.name));
        for t in s.sig.types() {
            x.push_str(&format!("      <arg type=\"{t}\"/>\n"));
        }
        x.push_str("    </signal>\n");
    }
    x.push_str("  </interface>\n");
}

/// `/etc/machine-id`, or the older `/var/lib/dbus/machine-id`.
pub(crate) fn machine_id() -> io::Result<String> {
    let s = std::fs::read_to_string("/etc/machine-id")
        .or_else(|_| std::fs::read_to_string("/var/lib/dbus/machine-id"))?;
    Ok(s.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    fn exports() -> Vec<Export> {
        let test = InterfaceSpec::new("mecha.Test")
            .method("Echo", "s", "s")
            .property("Count", "i", Access::Read, Value::I32(0))
            .signal("Changed", "s");
        vec![
            Export::new(ObjectPath::from("/mecha/Test"), test),
            Export::new(
                ObjectPath::from("/mecha/Other/Deep"),
                InterfaceSpec::new("mecha.Other"),
            ),
            Export::new(
                ObjectPath::from("/mechanix/x"),
                InterfaceSpec::new("mecha.X"),
            ),
        ]
    }

    #[test]
    fn children_are_one_level_deep_and_sorted() {
        let e = exports();
        let root = introspect(&ObjectPath::from("/"), e.iter());
        assert!(root.contains("  <node name=\"mecha\"/>\n  <node name=\"mechanix\"/>\n"));
        assert!(!root.contains("org.freedesktop.DBus.Properties"));
        let mecha = introspect(&ObjectPath::from("/mecha"), e.iter());
        assert!(mecha.contains("  <node name=\"Other\"/>\n  <node name=\"Test\"/>\n"));
        assert!(!mecha.contains("mechanix"), "/mechanix is not under /mecha");
    }

    #[test]
    fn an_interface_is_described() {
        let e = exports();
        let xml = introspect(&ObjectPath::from("/mecha/Test"), e.iter());
        assert!(xml.contains(
            "  <interface name=\"mecha.Test\">
    <method name=\"Echo\">
      <arg type=\"s\" direction=\"in\"/>
      <arg type=\"s\" direction=\"out\"/>
    </method>
    <property name=\"Count\" type=\"i\" access=\"read\"/>
    <signal name=\"Changed\">
      <arg type=\"s\"/>
    </signal>
  </interface>
"
        ));
        assert!(xml.contains("org.freedesktop.DBus.Properties"));
        assert!(xml.starts_with("<!DOCTYPE node"));
        assert!(xml.ends_with("</node>\n"));
    }
}
