include $(TOPDIR)/rules.mk

PKG_NAME:=ncut-autologin
PKG_VERSION:=4.0.0
PKG_RELEASE:=1
PKGARCH:=all

include $(INCLUDE_DIR)/package.mk

define Package/ncut-autologin
  SECTION:=net
  CATEGORY:=Network
  TITLE:=NCUT campus network auto login
  DEPENDS:=+curl +ca-bundle
endef

define Package/ncut-autologin/conffiles
/etc/config/ncut-autologin
endef

define Build/Compile
endef

define Package/ncut-autologin/install
	$(INSTALL_DIR) $(1)/usr/bin $(1)/etc/init.d $(1)/etc/config
	$(INSTALL_BIN) ./files/ncut-autologin.sh $(1)/usr/bin/ncut-autologin
	$(INSTALL_BIN) ./files/ncut-autologin.init $(1)/etc/init.d/ncut-autologin
	$(INSTALL_CONF) ./files/ncut-autologin.config $(1)/etc/config/ncut-autologin
	$(INSTALL_DIR) $(1)/www/luci-static/resources/view $(1)/usr/share/luci/menu.d $(1)/usr/share/rpcd/acl.d
	$(INSTALL_DATA) ./files/luci-view.js $(1)/www/luci-static/resources/view/ncut-autologin.js
	$(INSTALL_DATA) ./files/luci-menu.json $(1)/usr/share/luci/menu.d/ncut-autologin.json
	$(INSTALL_DATA) ./files/rpcd-acl.json $(1)/usr/share/rpcd/acl.d/ncut-autologin.json
endef

$(eval $(call BuildPackage,ncut-autologin))
