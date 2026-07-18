use crate::plugin::Plugin;

/// 返回所有编译时注册的插件工厂函数
pub fn factory_list() -> Vec<fn() -> Box<dyn Plugin>> {
    vec![]
}
