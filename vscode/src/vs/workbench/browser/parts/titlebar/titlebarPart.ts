/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

import 'vs/css!./media/titlebarpart';
import { localize } from 'vs/nls';
import { Part } from 'vs/workbench/browser/part';
import { ITitleService, ITitleProperties } from 'vs/workbench/services/title/common/titleService';
import { getZoomFactor, isWCOEnabled } from 'vs/base/browser/browser';
import { MenuBarVisibility, getTitleBarStyle, getMenuBarVisibility } from 'vs/platform/window/common/window';
import { IContextMenuService } from 'vs/platform/contextview/browser/contextView';
import { StandardMouseEvent } from 'vs/base/browser/mouseEvent';
import { IConfigurationService, IConfigurationChangeEvent } from 'vs/platform/configuration/common/configuration';
import { DisposableStore } from 'vs/base/common/lifecycle';
import { IBrowserWorkbenchEnvironmentService } from 'vs/workbench/services/environment/browser/environmentService';
import { IThemeService } from 'vs/platform/theme/common/themeService';
import { ThemeIcon } from 'vs/base/common/themables';
import { TITLE_BAR_ACTIVE_BACKGROUND, TITLE_BAR_ACTIVE_FOREGROUND, TITLE_BAR_INACTIVE_FOREGROUND, TITLE_BAR_INACTIVE_BACKGROUND, TITLE_BAR_BORDER, WORKBENCH_BACKGROUND } from 'vs/workbench/common/theme';
import { isMacintosh, isWindows, isLinux, isWeb, isNative, platformLocale } from 'vs/base/common/platform';
import { Color } from 'vs/base/common/color';
import { EventType, EventHelper, Dimension, append, $, addDisposableListener, prepend, reset } from 'vs/base/browser/dom';
import { CustomMenubarControl } from 'vs/workbench/browser/parts/titlebar/menubarControl';
import { IInstantiationService, ServicesAccessor } from 'vs/platform/instantiation/common/instantiation';
import { Emitter, Event } from 'vs/base/common/event';
import { IStorageService } from 'vs/platform/storage/common/storage';
import { Parts, IWorkbenchLayoutService, ActivityBarPosition, LayoutSettings } from 'vs/workbench/services/layout/browser/layoutService';
import { createActionViewItem, createAndFillInActionBarActions } from 'vs/platform/actions/browser/menuEntryActionViewItem';
import { Action2, IMenu, IMenuService, MenuId, registerAction2 } from 'vs/platform/actions/common/actions';
import { ContextKeyExpr, ContextKeyExpression, IContextKeyService } from 'vs/platform/contextkey/common/contextkey';
import { IHostService } from 'vs/workbench/services/host/browser/host';
import { Codicon } from 'vs/base/common/codicons';
import { getIconRegistry } from 'vs/platform/theme/common/iconRegistry';
import { WindowTitle } from 'vs/workbench/browser/parts/titlebar/windowTitle';
import { CommandCenterControl } from 'vs/workbench/browser/parts/titlebar/commandCenterControl';
import { IHoverDelegate } from 'vs/base/browser/ui/iconLabel/iconHoverDelegate';
import { IHoverService } from 'vs/workbench/services/hover/browser/hover';
import { Categories } from 'vs/platform/action/common/actionCommonCategories';
import { WorkbenchToolBar } from 'vs/platform/actions/browser/toolbar';
import { ACCOUNTS_ACTIVITY_ID, GLOBAL_ACTIVITY_ID } from 'vs/workbench/common/activity';
import { SimpleAccountActivityActionViewItem, SimpleGlobalActivityActionViewItem } from 'vs/workbench/browser/parts/globalCompositeBar';
import { HoverPosition } from 'vs/base/browser/ui/hover/hoverWidget';
import { IEditorGroupsService } from 'vs/workbench/services/editor/common/editorGroupsService';
import { ActionRunner, IAction } from 'vs/base/common/actions';
import { IEditorService } from 'vs/workbench/services/editor/common/editorService';
import { ActionsOrientation, IActionViewItem, prepareActions } from 'vs/base/browser/ui/actionbar/actionbar';
import { EDITOR_CORE_NAVIGATION_COMMANDS } from 'vs/workbench/browser/parts/editor/editorCommands';
import { AnchorAlignment } from 'vs/base/browser/ui/contextview/contextview';
import { EditorPane } from 'vs/workbench/browser/parts/editor/editorPane';
import { IKeybindingService } from 'vs/platform/keybinding/common/keybinding';
import { ResolvedKeybinding } from 'vs/base/common/keybindings';
import { EditorCommandsContextActionRunner } from 'vs/workbench/browser/parts/editor/editorTabsControl';
import { IEditorCommandsContext, IToolbarActions } from 'vs/workbench/common/editor';
import { mainWindow } from 'vs/base/browser/window';

export class TitlebarPart extends Part implements ITitleService {

	declare readonly _serviceBrand: undefined;

	//#region IView

	readonly minimumWidth: number = 0;
	readonly maximumWidth: number = Number.POSITIVE_INFINITY;
	get minimumHeight(): number {
		const value = this.isCommandCenterVisible || (isWeb && isWCOEnabled()) ? 35 : 30;
		return value / (this.useCounterZoom ? getZoomFactor() : 1);
	}

	get maximumHeight(): number { return this.minimumHeight; }

	//#endregion

	private _onMenubarVisibilityChange = this._register(new Emitter<boolean>());
	readonly onMenubarVisibilityChange = this._onMenubarVisibilityChange.event;

	private readonly _onDidChangeCommandCenterVisibility = new Emitter<void>();
	readonly onDidChangeCommandCenterVisibility: Event<void> = this._onDidChangeCommandCenterVisibility.event;

	protected rootContainer!: HTMLElement;
	protected primaryWindowControls: HTMLElement | undefined;
	protected dragRegion: HTMLElement | undefined;
	protected title!: HTMLElement;

	private leftContent!: HTMLElement;
	private centerContent!: HTMLElement;
	private rightContent!: HTMLElement;

	protected customMenubar: CustomMenubarControl | undefined;
	protected appIcon: HTMLElement | undefined;
	private appIconBadge: HTMLElement | undefined;
	protected menubar?: HTMLElement;
	protected lastLayoutDimensions: Dimension | undefined;

	private actionToolBar!: WorkbenchToolBar;
	private actionToolBarDisposable = this._register(new DisposableStore());
	private editorActionsChangeDisposable = this._register(new DisposableStore());
	private actionToolBarElement!: HTMLElement;

	private layoutToolbarMenu: IMenu | undefined;
	private readonly editorToolbarMenuDisposables = this._register(new DisposableStore());
	private readonly layoutToolbarMenuDisposables = this._register(new DisposableStore());

	private hoverDelegate: IHoverDelegate;

	private readonly titleDisposables = this._register(new DisposableStore());
	private titleBarStyle: 'native' | 'custom';

	private isInactive: boolean = false;

	private readonly windowTitle: WindowTitle;

	constructor(
		@IContextMenuService private readonly contextMenuService: IContextMenuService,
		@IConfigurationService protected readonly configurationService: IConfigurationService,
		@IBrowserWorkbenchEnvironmentService protected readonly environmentService: IBrowserWorkbenchEnvironmentService,
		@IInstantiationService protected readonly instantiationService: IInstantiationService,
		@IThemeService themeService: IThemeService,
		@IStorageService storageService: IStorageService,
		@IWorkbenchLayoutService layoutService: IWorkbenchLayoutService,
		@IContextKeyService private readonly contextKeyService: IContextKeyService,
		@IHostService private readonly hostService: IHostService,
		@IHoverService hoverService: IHoverService,
		@IEditorGroupsService private editorGroupService: IEditorGroupsService,
		@IEditorService private editorService: IEditorService,
		@IMenuService private readonly menuService: IMenuService,
		@IKeybindingService private readonly keybindingService: IKeybindingService,
	) {
		super(Parts.TITLEBAR_PART, { hasTitle: false }, themeService, storageService, layoutService);
		this.windowTitle = this._register(instantiationService.createInstance(WindowTitle, mainWindow, 'main'));

		this.titleBarStyle = getTitleBarStyle(this.configurationService);

		this.hoverDelegate = new class implements IHoverDelegate {

			private _lastHoverHideTime: number = 0;

			readonly showHover = hoverService.showHover.bind(hoverService);
			readonly placement = 'element';

			get delay(): number {
				return Date.now() - this._lastHoverHideTime < 200
					? 0  // show instantly when a hover was recently shown
					: configurationService.getValue<number>('workbench.hover.delay');
			}

			onDidHideHover() {
				this._lastHoverHideTime = Date.now();
			}
		};

		this.registerListeners();
	}

	updateProperties(properties: ITitleProperties): void {
		this.windowTitle.updateProperties(properties);
	}

	get isCommandCenterVisible() {
		return this.configurationService.getValue<boolean>(LayoutSettings.COMMAND_CENTER);
	}

	private registerListeners(): void {
		this._register(this.hostService.onDidChangeFocus(focused => focused ? this.onFocus() : this.onBlur()));
		this._register(this.configurationService.onDidChangeConfiguration(e => this.onConfigurationChanged(e)));
	}

	private onBlur(): void {
		this.isInactive = true;
		this.updateStyles();
	}

	private onFocus(): void {
		this.isInactive = false;
		this.updateStyles();
	}

	protected onConfigurationChanged(event: IConfigurationChangeEvent): void {

		if (this.titleBarStyle !== 'native' && (!isMacintosh || isWeb)) {
			if (event.affectsConfiguration('window.menuBarVisibility')) {
				if (this.currentMenubarVisibility === 'compact') {
					this.uninstallMenubar();
				} else {
					this.installMenubar();
				}
			}
		}

		if (this.titleBarStyle !== 'native' && this.actionToolBar) {
			const affectsEditorActions = event.affectsConfiguration('workbench.editor.showEditorActionsInTitleBar') || event.affectsConfiguration('workbench.editor.showTabs');
			const affectsLayoutControl = event.affectsConfiguration('workbench.layoutControl.enabled');
			const affectsActivityControl = event.affectsConfiguration(LayoutSettings.ACTIVITY_BAR_LOCATION);
			if (affectsEditorActions) {
				this.createActionToolBar();
			}
			if (affectsEditorActions || affectsLayoutControl || affectsActivityControl) {
				this.createActionToolBarMenus({ editorActions: affectsEditorActions, layoutActions: affectsLayoutControl, activityActions: affectsActivityControl });
				this._onDidChange.fire(undefined);
			}
		}

		if (event.affectsConfiguration(LayoutSettings.COMMAND_CENTER)) {
			this.updateTitle();
			this._onDidChangeCommandCenterVisibility.fire();
			this._onDidChange.fire(undefined);
		}
	}

	protected onMenubarVisibilityChanged(visible: boolean): void {
		if (isWeb || isWindows || isLinux) {
			if (this.lastLayoutDimensions) {
				this.layout(this.lastLayoutDimensions.width, this.lastLayoutDimensions.height);
			}

			this._onMenubarVisibilityChange.fire(visible);
		}
	}


	private uninstallMenubar(): void {
		if (this.customMenubar) {
			this.customMenubar.dispose();
			this.customMenubar = undefined;
		}

		if (this.menubar) {
			this.menubar.remove();
			this.menubar = undefined;
		}

		this.onMenubarVisibilityChanged(false);
	}

	protected installMenubar(): void {
		// If the menubar is already installed, skip
		if (this.menubar) {
			return;
		}

		this.customMenubar = this._register(this.instantiationService.createInstance(CustomMenubarControl));

		this.menubar = append(this.leftContent, $('div.menubar'));
		this.menubar.setAttribute('role', 'menubar');

		this._register(this.customMenubar.onVisibilityChange(e => this.onMenubarVisibilityChanged(e)));

		this.customMenubar.create(this.menubar);
	}

	private updateTitle(): void {
		this.titleDisposables.clear();
		if (!this.isCommandCenterVisible) {
			// Text Title
			this.title.innerText = this.windowTitle.value;
			this.titleDisposables.add(this.windowTitle.onDidChange(() => {
				this.title.innerText = this.windowTitle.value;
			}));
		} else {
			// Menu Title
			const commandCenter = this.instantiationService.createInstance(CommandCenterControl, this.windowTitle, this.hoverDelegate);
			reset(this.title, commandCenter.element);
			this.titleDisposables.add(commandCenter);
		}
	}

	protected override createContentArea(parent: HTMLElement): HTMLElement {
		this.element = parent;
		this.rootContainer = append(parent, $('.titlebar-container'));

		this.leftContent = append(this.rootContainer, $('.titlebar-left'));
		this.centerContent = append(this.rootContainer, $('.titlebar-center'));
		this.rightContent = append(this.rootContainer, $('.titlebar-right'));

		// App Icon (Native Windows/Linux and Web)
		if (!isMacintosh && !isWeb) {
			this.appIcon = prepend(this.leftContent, $('a.window-appicon'));

			// Web-only home indicator and menu
			if (isWeb) {
				const homeIndicator = this.environmentService.options?.homeIndicator;
				if (homeIndicator) {
					const icon: ThemeIcon = getIconRegistry().getIcon(homeIndicator.icon) ? { id: homeIndicator.icon } : Codicon.code;

					this.appIcon.setAttribute('href', homeIndicator.href);
					this.appIcon.classList.add(...ThemeIcon.asClassNameArray(icon));
					this.appIconBadge = document.createElement('div');
					this.appIconBadge.classList.add('home-bar-icon-badge');
					this.appIcon.appendChild(this.appIconBadge);
				}
			}
		}

		// Draggable region that we can manipulate for #52522
		this.dragRegion = prepend(this.rootContainer, $('div.titlebar-drag-region'));

		// Menubar: install a custom menu bar depending on configuration
		// and when not in activity bar
		if (this.titleBarStyle !== 'native'
			&& (!isMacintosh || isWeb)
			&& this.currentMenubarVisibility !== 'compact') {
			this.installMenubar();
		}

		// Title
		this.title = append(this.centerContent, $('div.window-title'));
		this.updateTitle();

		if (this.titleBarStyle !== 'native') {
			// Create Toolbar Actions
			this.actionToolBarElement = append(this.rightContent, $('div.action-toolbar-container'));
			this.createActionToolBar();
			this.createActionToolBarMenus();
		}

		let primaryControlLocation = isMacintosh ? 'left' : 'right';
		if (isMacintosh && isNative) {
			// Check if the locale is RTL, macOS will move traffic lights in RTL locales
			// https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Intl/Locale/textInfo
			const localeInfo = new Intl.Locale(platformLocale) as any;
			if (localeInfo?.textInfo?.direction === 'rtl') {
				primaryControlLocation = 'right';
			}
		}

		this.primaryWindowControls = append(primaryControlLocation === 'left' ? this.leftContent : this.rightContent, $('div.window-controls-container.primary'));
		append(primaryControlLocation === 'left' ? this.rightContent : this.leftContent, $('div.window-controls-container.secondary'));

		// Context menu on title
		[EventType.CONTEXT_MENU, EventType.MOUSE_DOWN].forEach(event => {
			this._register(addDisposableListener(this.rootContainer, event, e => {
				if (e.type === EventType.CONTEXT_MENU || (e.target === this.title && e.metaKey)) {
					EventHelper.stop(e);
					this.onContextMenu(e, e.target === this.title ? MenuId.TitleBarTitleContext : MenuId.TitleBarContext);
				}
			}));
		});

		this.updateStyles();

		const that = this;
		registerAction2(class FocusTitleBar extends Action2 {

			constructor() {
				super({
					id: `workbench.action.focusTitleBar`,
					title: { value: localize('focusTitleBar', "Focus Title Bar"), original: 'Focus Title Bar' },
					category: Categories.View,
					f1: true,
				});
			}

			run(): void {
				if (that.customMenubar) {
					that.customMenubar.toggleFocus();
				} else {
					(that.element.querySelector('[tabindex]:not([tabindex="-1"])') as HTMLElement).focus();
				}
			}
		});

		return this.element;
	}

	private actionViewItemProvider(action: IAction): IActionViewItem | undefined {
		// --- Activity Actions
		if (action.id === GLOBAL_ACTIVITY_ID) {
			return this.instantiationService.createInstance(SimpleGlobalActivityActionViewItem, { position: () => HoverPosition.BELOW });
		}
		if (action.id === ACCOUNTS_ACTIVITY_ID) {
			return this.instantiationService.createInstance(SimpleAccountActivityActionViewItem, { position: () => HoverPosition.BELOW });
		}

		// --- Editor Actions
		const activeEditorPane = this.editorGroupService.activeGroup?.activeEditorPane;
		if (activeEditorPane && activeEditorPane instanceof EditorPane) {
			const result = activeEditorPane.getActionViewItem(action);

			if (result) {
				return result;
			}
		}

		// Check extensions
		return createActionViewItem(this.instantiationService, action, { hoverDelegate: this.hoverDelegate, menuAsChild: false });
	}

	protected getKeybinding(action: IAction): ResolvedKeybinding | undefined {
		const editorPaneAwareContextKeyService = this.editorGroupService.activeGroup?.activeEditorPane?.scopedContextKeyService ?? this.contextKeyService;
		return this.keybindingService.lookupKeybinding(action.id, editorPaneAwareContextKeyService);
	}

	private createActionToolBar() {
		// Creates the action tool bar. Depends on the configuration of the title bar menus
		// Requires to be recreated whenever editor actions enablement changes

		this.actionToolBarDisposable.clear();

		this.actionToolBar = this.instantiationService.createInstance(WorkbenchToolBar, this.actionToolBarElement, {
			contextMenu: MenuId.TitleBarContext,
			orientation: ActionsOrientation.HORIZONTAL,
			ariaLabel: localize('ariaLabelTitleActions', "Title actions"),
			getKeyBinding: action => this.getKeybinding(action),
			overflowBehavior: { maxItems: 9, exempted: [ACCOUNTS_ACTIVITY_ID, GLOBAL_ACTIVITY_ID, ...EDITOR_CORE_NAVIGATION_COMMANDS] },
			anchorAlignmentProvider: () => AnchorAlignment.RIGHT,
			telemetrySource: 'titlePart',
			highlightToggledItems: this.editorActionsEnabled, // Only show toggled state for editor actions (Layout actions are not shown as toggled)
			actionViewItemProvider: action => this.actionViewItemProvider(action)
		});

		this.actionToolBarDisposable.add(this.actionToolBar);

		if (this.editorActionsEnabled) {
			this.actionToolBarDisposable.add(this.editorGroupService.onDidChangeActiveGroup(() => this.createActionToolBarMenus({ editorActions: true })));
		}
	}

	private createActionToolBarMenus(update: true | { editorActions?: boolean; layoutActions?: boolean; activityActions?: boolean } = true) {
		if (update === true) {
			update = { editorActions: true, layoutActions: true, activityActions: true };
		}

		const updateToolBarActions = () => {
			const actions: IToolbarActions = { primary: [], secondary: [] };

			// --- Editor Actions
			if (this.editorActionsEnabled) {
				this.editorActionsChangeDisposable.clear();

				const activeGroup = this.editorGroupService.activeGroup;
				if (activeGroup) { // Can be undefined on startup
					const editorActions = activeGroup.createEditorActions(this.editorActionsChangeDisposable);

					actions.primary.push(...editorActions.actions.primary);
					actions.secondary.push(...editorActions.actions.secondary);

					this.editorActionsChangeDisposable.add(editorActions.onDidChange(() => updateToolBarActions()));
				}
			}

			// --- Layout Actions
			if (this.layoutToolbarMenu) {
				createAndFillInActionBarActions(
					this.layoutToolbarMenu,
					{},
					actions,
					() => !this.editorActionsEnabled // Layout Actions in overflow menu when editor actions enabled in title bar
				);
			}

			// --- Activity Actions
			if (this.activityActionsEnabled) {
				actions.primary.push(ACCOUNTS_ACTIVITY_TILE_ACTION);
				actions.primary.push(GLOBAL_ACTIVITY_TITLE_ACTION);
			}

			this.actionToolBar.setActions(prepareActions(actions.primary), prepareActions(actions.secondary));
		};

		// Create/Update the menus which should be in the title tool bar

		if (update.editorActions) {
			this.editorToolbarMenuDisposables.clear();

			// The editor toolbar menu is handled by the editor group so we do not need to manage it here.
			// However, depending on the active editor, we need to update the context and action runner of the toolbar menu.
			if (this.editorActionsEnabled && this.editorService.activeEditor !== undefined) {
				const context: IEditorCommandsContext = { groupId: this.editorGroupService.activeGroup.id };

				this.actionToolBar.actionRunner = new EditorCommandsContextActionRunner(context);
				this.actionToolBar.context = context;
				this.editorToolbarMenuDisposables.add(this.actionToolBar.actionRunner);
			} else {
				this.actionToolBar.actionRunner = new ActionRunner();
				this.actionToolBar.context = {};

				this.editorToolbarMenuDisposables.add(this.actionToolBar.actionRunner);
			}
		}

		if (update.layoutActions) {
			this.layoutToolbarMenuDisposables.clear();

			if (this.layoutControlEnabled) {
				this.layoutToolbarMenu = this.menuService.createMenu(MenuId.LayoutControlMenu, this.contextKeyService);

				this.layoutToolbarMenuDisposables.add(this.layoutToolbarMenu);
				this.layoutToolbarMenuDisposables.add(this.layoutToolbarMenu.onDidChange(() => updateToolBarActions()));
			} else {
				this.layoutToolbarMenu = undefined;
			}
		}

		updateToolBarActions();
	}

	override updateStyles(): void {
		super.updateStyles();

		// Part container
		if (this.element) {
			if (this.isInactive) {
				this.element.classList.add('inactive');
			} else {
				this.element.classList.remove('inactive');
			}

			const titleBackground = this.getColor(this.isInactive ? TITLE_BAR_INACTIVE_BACKGROUND : TITLE_BAR_ACTIVE_BACKGROUND, (color, theme) => {
				// LCD Rendering Support: the title bar part is a defining its own GPU layer.
				// To benefit from LCD font rendering, we must ensure that we always set an
				// opaque background color. As such, we compute an opaque color given we know
				// the background color is the workbench background.
				return color.isOpaque() ? color : color.makeOpaque(WORKBENCH_BACKGROUND(theme));
			}) || '';
			this.element.style.backgroundColor = titleBackground;

			if (this.appIconBadge) {
				this.appIconBadge.style.backgroundColor = titleBackground;
			}

			if (titleBackground && Color.fromHex(titleBackground).isLighter()) {
				this.element.classList.add('light');
			} else {
				this.element.classList.remove('light');
			}

			const titleForeground = this.getColor(this.isInactive ? TITLE_BAR_INACTIVE_FOREGROUND : TITLE_BAR_ACTIVE_FOREGROUND);
			this.element.style.color = titleForeground || '';

			const titleBorder = this.getColor(TITLE_BAR_BORDER);
			this.element.style.borderBottom = titleBorder ? `1px solid ${titleBorder}` : '';
		}
	}

	protected onContextMenu(e: MouseEvent, menuId: MenuId): void {
		// Find target anchor
		const event = new StandardMouseEvent(e);

		// Show it
		this.contextMenuService.showContextMenu({
			getAnchor: () => event,
			menuId,
			contextKeyService: this.contextKeyService,
			domForShadowRoot: isMacintosh && isNative ? event.target : undefined
		});
	}

	protected get currentMenubarVisibility(): MenuBarVisibility {
		return getMenuBarVisibility(this.configurationService);
	}

	private get layoutControlEnabled(): boolean {
		return this.configurationService.getValue<boolean>('workbench.layoutControl.enabled');
	}

	private get editorActionsEnabled(): boolean {
		return this.editorGroupService.partOptions.showEditorActionsInTitleBar !== 'never' && this.editorGroupService.partOptions.showTabs === 'none';
	}

	private get activityActionsEnabled(): boolean {
		return this.configurationService.getValue(LayoutSettings.ACTIVITY_BAR_LOCATION) === ActivityBarPosition.TOP;
	}

	protected get useCounterZoom(): boolean {
		// Prevent zooming behavior if any of the following conditions are met:
		// 1. Shrinking below the window control size (zoom < 1)
		// 2. No custom items are present in the title bar
		const zoomFactor = getZoomFactor();

		const noMenubar = this.currentMenubarVisibility === 'hidden' || (!isWeb && isMacintosh);
		const noCommandCenter = !this.isCommandCenterVisible;
		const noLayoutControls = !this.layoutControlEnabled;
		return zoomFactor < 1 || (noMenubar && noCommandCenter && noLayoutControls);
	}

	updateLayout(dimension: Dimension): void {
		this.lastLayoutDimensions = dimension;

		if (getTitleBarStyle(this.configurationService) === 'custom') {
			const zoomFactor = getZoomFactor();

			this.element.style.setProperty('--zoom-factor', zoomFactor.toString());
			this.rootContainer.classList.toggle('counter-zoom', this.useCounterZoom);

			if (this.customMenubar) {
				const menubarDimension = new Dimension(0, dimension.height);
				this.customMenubar.layout(menubarDimension);
			}
		}
	}

	override layout(width: number, height: number): void {
		this.updateLayout(new Dimension(width, height));

		super.layoutContents(width, height);
	}

	toJSON(): object {
		return {
			type: Parts.TITLEBAR_PART
		};
	}
}


class ToogleConfigAction extends Action2 {

	constructor(private readonly section: string, title: string, order: number, when?: ContextKeyExpression) {
		super({
			id: `toggle.${section}`,
			title,
			toggled: ContextKeyExpr.equals(`config.${section}`, true),
			menu: { id: MenuId.TitleBarContext, order, when }
		});
	}

	run(accessor: ServicesAccessor, ...args: any[]): void {
		const configService = accessor.get(IConfigurationService);
		const value = configService.getValue(this.section);
		configService.updateValue(this.section, !value);
	}
}

registerAction2(class ToogleCommandCenter extends ToogleConfigAction {
	constructor() {
		super(LayoutSettings.COMMAND_CENTER, localize('toggle.commandCenter', 'Command Center'), 1);
	}
});

registerAction2(class ToogleLayoutControl extends ToogleConfigAction {
	constructor() {
		super('workbench.layoutControl.enabled', localize('toggle.layout', 'Layout Controls'), 2);
	}
});

registerAction2(class ToogleEditorActionsControl extends ToogleConfigAction {
	constructor() {
		super('workbench.editor.showEditorActionsInTitleBar', localize('toggle.editorActions', 'Editor Actions'), 2, ContextKeyExpr.equals('config.workbench.editor.showTabs', 'none'));
	}
});

const ACCOUNTS_ACTIVITY_TILE_ACTION: IAction = {
	id: ACCOUNTS_ACTIVITY_ID,
	label: localize('accounts', "Accounts"),
	tooltip: localize('accounts', "Accounts"),
	class: undefined,
	enabled: true,
	run: function (): void { }
};

const GLOBAL_ACTIVITY_TITLE_ACTION: IAction = {
	id: GLOBAL_ACTIVITY_ID,
	label: localize('manage', "Manage"),
	tooltip: localize('manage', "Manage"),
	class: undefined,
	enabled: true,
	run: function (): void { }
};
