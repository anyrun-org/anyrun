use std::{path::PathBuf, sync::Arc};

use abi_stable::std_types::{ROption, RVec};
use anyrun_interface::{Match, PluginInfo};
use gtk::{pango, prelude::*};
use gtk4 as gtk;
use relm4::prelude::*;

use crate::Config;

pub struct PluginMatch {
    pub content: Match,
    pub row: gtk::ListBoxRow,
    config: Arc<Config>,
}

#[relm4::factory(pub)]
impl FactoryComponent for PluginMatch {
    type Init = (Match, Arc<Config>);
    type Input = ();
    type Output = ();
    type CommandOutput = ();
    type ParentWidget = gtk::ListBox;
    view! {
        gtk::ListBoxRow {
            set_css_classes: &["match"],
            set_height_request: 32,
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 10,
                set_css_classes: &["match"],
                set_hexpand: true,

                #[name = "icon"]
                gtk::Image {
                    set_pixel_size: 32,
                    set_visible: false,
                    set_css_classes: &["match"]
                },

                #[name = "text"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_css_classes: &["match", "text-fields"],
                    set_valign: gtk::Align::Center,
                    set_hexpand: true,
                    set_vexpand: true,

                    gtk::Label {
                        set_css_classes: &["match", "title"],
                        set_halign: gtk::Align::Start,
                        set_valign: gtk::Align::Center,
                        set_xalign: 0.0,
                        set_wrap: true,
                        set_natural_wrap_mode: gtk::NaturalWrapMode::Word,
                        set_wrap_mode: pango::WrapMode::WordChar,
                        set_use_markup: self.content.use_pango,
                        set_label: &self.content.title,
                    },

                    #[name = "description"]
                    gtk::Label {
                        set_css_classes: &["match", "description"],
                        set_wrap: true,
                        set_xalign: 0.0,
                        set_use_markup: self.content.use_pango,
                        set_halign: gtk::Align::Start,
                        set_valign: gtk::Align::Center,
                    }
                }
            }
        }
    }

    fn init_widgets(
        &mut self,
        _index: &Self::Index,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        _sender: FactorySender<Self>,
    ) -> Self::Widgets {
        let widgets = view_output!();

        self.row = root;

        if !self.config.hide_icons {
            if let ROption::RSome(icon) = &self.content.icon {
                widgets.icon.set_visible(true);
                let path = PathBuf::from(icon.to_string());
                if path.is_absolute() {
                    widgets.icon.set_from_file(Some(path));
                } else {
                    widgets.icon.set_icon_name(Some(icon));
                }
            }
        }

        match &self.content.description {
            ROption::RSome(desc) => widgets.description.set_label(desc),
            ROption::RNone => widgets.description.set_visible(false),
        }

        widgets
    }

    fn init_model(
        (content, config): Self::Init,
        _index: &Self::Index,
        _sender: FactorySender<Self>,
    ) -> Self {
        let row = gtk::ListBoxRow::default();

        Self {
            row,
            content,
            config,
        }
    }
}

pub struct PluginBox {
    pub plugin_info: PluginInfo,
    pub matches: FactoryVecDeque<PluginMatch>,
    config: Arc<Config>,
    visible: bool,
    enabled: bool,
}

#[derive(Debug, Clone)]
pub enum PluginBoxInput {
    Matches(RVec<Match>),
    Enable(bool),
}

#[derive(Debug)]
pub enum PluginBoxOutput {
    MatchesLoaded,
    RowSelected(<PluginBox as FactoryComponent>::Index),
}

#[relm4::factory(pub)]
impl FactoryComponent for PluginBox {
    type Init = (PluginInfo, Arc<Config>);
    type Input = PluginBoxInput;
    type Output = PluginBoxOutput;
    type CommandOutput = (u64, RVec<Match>);
    type ParentWidget = gtk::Box;

    view! {
        gtk::Box {
            #[watch]
            set_visible: self.visible,
            set_css_classes: &["plugin"],

            gtk::Box {
                set_visible: !self.config.hide_plugin_info,
                set_css_classes: &["plugin", "info"],
                set_orientation: gtk::Orientation::Vertical,

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_expand: false,

                    gtk::Image {
                        set_css_classes: &["plugin", "info"],
                        set_icon_name: Some(&self.plugin_info.icon),
                        set_visible: !self.config.hide_icons,
                        set_halign: gtk::Align::Start,
                        set_valign: gtk::Align::Start,
                        set_pixel_size: 32,
                    },
                    gtk::Label {
                        set_css_classes: &["plugin", "info"],
                        set_label: &self.plugin_info.name,
                        set_halign: gtk::Align::Start,
                        set_valign: gtk::Align::Center,
                    }
                }
            },

            #[local_ref]
            matches -> gtk::ListBox {
                set_css_classes: &["plugin"],
                set_hexpand: true,
                connect_row_selected[index] => move |_list, row| {
                    if row.is_some() {
                        sender.output(PluginBoxOutput::RowSelected(index.clone())).unwrap();
                    }
                }
            }
        }
    }

    fn init_widgets(
        &mut self,
        index: &Self::Index,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        sender: FactorySender<Self>,
    ) -> Self::Widgets {
        let matches = self.matches.widget();

        let widgets = view_output!();

        widgets
    }

    fn init_model(
        (plugin_info, config): Self::Init,
        _index: &Self::Index,
        _sender: FactorySender<Self>,
    ) -> Self {
        let matches = FactoryVecDeque::builder()
            .launch(gtk::ListBox::default())
            .detach();

        Self {
            plugin_info,
            matches,
            config,
            visible: false,
            enabled: true,
        }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::Input,
        sender: FactorySender<Self>,
    ) {
        match message {
            PluginBoxInput::Matches(matches) => {
                if !self.enabled {
                    return;
                }

                self.visible = !matches.is_empty();
                {
                    let mut guard = self.matches.guard();

                    guard.clear();

                    for _match in matches {
                        guard.push_back((_match, self.config.clone()));
                    }
                }
                sender.output(PluginBoxOutput::MatchesLoaded).unwrap();
            }
            PluginBoxInput::Enable(enable) => {
                self.enabled = enable;
                self.visible = enable;

                if !enable {
                    self.matches.guard().clear();
                }
            }
        }

        self.update_view(widgets, sender);
    }
}
