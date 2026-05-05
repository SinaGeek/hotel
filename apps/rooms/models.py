from django.db import models
from django.core.exceptions import ValidationError
from apps.core.models import HotelScopedModel, SoftDeleteModel

class RoomType(HotelScopedModel):
    name_en = models.CharField(max_length=80, verbose_name="Name (EN)", default="")
    name_tr = models.CharField(max_length=80, verbose_name="Name (TR)", default="")
    name_fa = models.CharField(max_length=80, verbose_name="نام (FA)", default="")
    base_rate=models.DecimalField(max_digits=10, decimal_places=2, verbose_name="نرخ پایه", default=0)

    class Meta:
        verbose_name = "نوع اتاق"
        verbose_name_plural = "انواع اتاق"
        
    def __str__(self):
        from django.utils.translation import get_language
        lang = get_language()
        if lang == 'fa': return self.name_fa
        if lang == 'tr': return self.name_tr
        return self.name_en

    @property
    def name(self):
        return self.__str__()

class RoomStatus(models.TextChoices):
    VACANT='VACANT'; OCCUPIED='OCCUPIED'; DIRTY='DIRTY'; CLEAN='CLEAN'; RESERVED='RESERVED'; OUT_OF_ORDER='OUT_OF_ORDER'

class Room(HotelScopedModel, SoftDeleteModel):
    number=models.CharField(max_length=10, verbose_name="شماره اتاق")
    floor=models.IntegerField(default=1, verbose_name="طبقه")
    room_type=models.ForeignKey(RoomType,on_delete=models.PROTECT, verbose_name="نوع اتاق", related_name='rooms')
    status=models.CharField(max_length=20, choices=RoomStatus.choices, default=RoomStatus.CLEAN, verbose_name="وضعیت")
    capacity=models.IntegerField(default=2, verbose_name="ظرفیت")
    is_clean = models.BooleanField(default=True, verbose_name="نظافت شده")
    
    def clean(self):
        super().clean()
        if not self.pk and Room.objects.filter(hotel_id=self.hotel_id).count() >= 500: # Increased limit
            raise ValidationError("Limit reached.")
            
    def save(self, *args, **kwargs):
        self.clean()
        super().save(*args, **kwargs)

    class Meta: 
        unique_together=(('hotel_id','number'),)
        verbose_name = "اتاق"
        verbose_name_plural = "اتاق‌ها"

class Table(HotelScopedModel):
    number=models.CharField(max_length=10, verbose_name="شماره میز")
    capacity=models.IntegerField(choices=[(2, '2 نفر'), (4, '4 نفر'), (6, '6 نفر')], default=2, verbose_name="ظرفیت")
    status=models.CharField(max_length=20, choices=[('vacant','خالی'), ('occupied','پر'), ('reserved','رزرو')], default='vacant', verbose_name="وضعیت")

    class Meta:
        verbose_name = "میز"
        verbose_name_plural = "میزها"
