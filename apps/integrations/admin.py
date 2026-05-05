from django.contrib import admin
from .models import IntegrationAccount, IntegrationEvent, DoorLockDevice, PointOfSaleDevice

admin.site.register(IntegrationAccount)
admin.site.register(IntegrationEvent)

@admin.register(DoorLockDevice)
class DoorLockDeviceAdmin(admin.ModelAdmin):
    list_display = ('name', 'room', 'ip_address', 'status')

@admin.register(PointOfSaleDevice)
class PointOfSaleDeviceAdmin(admin.ModelAdmin):
    list_display = ('name', 'device_type', 'ip_address', 'status')
